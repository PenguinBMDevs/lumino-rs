//! 工程走带视图的 egui 绘制（第一步：顶部 28px 标尺）。
//!
//! 与 iced `ui-editor/src/arrangement/ruler.rs` 同口径：
//! - 底色 `ruler_background_color`、底部 1px 边框；
//! - 每个小节边界画一条 1px 竖线（贯穿整条标尺）；
//! - 小节号画在边界右侧 4px、顶部 4px；相邻标签间距小于 38px 时只画线不画字；
//! - 文本字号 `MEASURE_NUMBER_FONT_SIZE = 12.5`。
//!
//! 坐标约定：`tick_to_x = tick * zoom_x - scroll_x`（与 wgpu 网格同源）。

pub mod lane;
mod measures;
mod track_list;

use egui::{Align2, FontId, Rect};

use crate::theme;

use super::{ArrangementModel, ArrangementRulerModel};

/// 标尺高度（生产 `ARRANGEMENT_RULER_HEIGHT`）。
pub const RULER_HEIGHT: f32 = 28.0;
/// 相邻小节号标签的最小间距（像素）；更密时只画刻度线。
const MIN_LABEL_SPACING: f32 = 38.0;
/// 小节号字号（生产 `MEASURE_NUMBER_FONT_SIZE`）。
const LABEL_SIZE: f32 = 12.5;
/// 标签相对边界的内缩。
const LABEL_PAD: f32 = 4.0;

/// 走带视图的布局常量（与 `arrangement_geometry` 同源）。
const TRACK_LIST_W: f32 = 160.0;
/// 垂直滚动条宽度。
const V_SCROLLBAR_W: f32 = 20.0;
/// 底部状态栏高度。
const STATUSBAR_H: f32 = 20.0;
/// 底部水平滚动条高度。
const H_SCROLLBAR_H: f32 = 20.0;

/// 本帧的轨道列动作。
#[derive(Debug, Clone, PartialEq)]
pub enum ArrangementAction {
    /// 选择一组音轨（已按 Shift 语义算好）。
    Select {
        /// 点击的音轨 ID（单选 / 双击语义）。
        id: usize,
        /// 选择集合（单选时只含 `id`）。
        ids: Vec<usize>,
    },
    /// 切换静音。
    ToggleMute(usize),
    /// 切换独奏。
    ToggleSolo(usize),
    /// 拖拽移动（列表局部坐标，已加滚动偏移）。
    ReorderMoved {
        /// 局部 x。
        x: f32,
        /// 局部 y。
        y: f32,
    },
    /// 结束拖拽（排序由生产侧按内部插入位置执行）。
    ReorderEnded,
    /// 标尺点击/拖拽：设置播放光标到该 tick。
    CursorSet(f64),
    /// 标尺滚轮：横向缩放（`fixed_ratio` = 指针在标尺内的相对位置 0..1）。
    ZoomX {
        /// 目标缩放（每 tick 像素）。
        zoom: f32,
        /// 缩放锚点比例。
        fixed_ratio: f32,
    },
}

/// 动作通道（一帧一条）。
const ACTION_ID: &str = "lumino.arrangement.action";

/// 记录本帧动作。
pub fn push_action(ctx: &egui::Context, action: ArrangementAction) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), action));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<ArrangementAction> {
    let action = ctx.data(|d| d.get_temp::<ArrangementAction>(egui::Id::unique(ACTION_ID)));
    if action.is_some() {
        ctx.data_mut(|d| d.remove::<ArrangementAction>(egui::Id::unique(ACTION_ID)));
    }
    action
}

/// 绘制并处理输入（顶部标尺 + 左侧轨道列）。
///
/// `full` = 窗口逻辑矩形；内容原点由模型给出（工具栏高度可拖拽，不能硬编码）。
/// 轨道列整块计入 egui 占用区：点击不再落到下方 iced 的同位置 Canvas。
pub fn show(ui: &egui::Ui, full: Rect, model: &ArrangementModel) -> Option<ArrangementAction> {
    let left = full.left() + model.ruler.left_inset;
    let top = full.top() + model.ruler.top_inset;
    let right = (full.right() - V_SCROLLBAR_W).max(left);
    let bottom = (full.bottom() - STATUSBAR_H - H_SCROLLBAR_H).max(top);

    let ruler = Rect::from_min_max(
        egui::pos2(left + TRACK_LIST_W, top),
        egui::pos2(right.max(left + TRACK_LIST_W), top + RULER_HEIGHT),
    );
    show_ruler(ui, ruler, &model.ruler);
    // 标尺由 egui 接管：整条计入占用区，避免 iced 再设一次光标。
    super::menu::extend_blocked(ui.ctx(), [ruler]);

    let lanes = Rect::from_min_max(
        egui::pos2(left, ruler.bottom()),
        egui::pos2(left + TRACK_LIST_W, bottom.max(ruler.bottom())),
    );
    track_list::show(ui, lanes, &model.track_list);
    // 轨道列由 egui 接管点击：整块占用，避免 iced 同位置 Canvas 二次响应
    // （两边选择逻辑不同步时会出现“点了又跳回去”）。
    super::menu::extend_blocked(ui.ctx(), [lanes]);

    // 标尺点击/拖拽设光标、滚轮缩放。
    if let Some(action) = ruler_input(ui, ruler, &model.ruler) {
        return Some(action);
    }
    track_list::interact(ui, lanes, &model.track_list)
}

/// 计算某 tick 对应的屏幕 x（`rect` 左缘 = 走带区左缘）。
pub fn tick_to_x(rect: Rect, model: &ArrangementRulerModel, tick: f32) -> f32 {
    rect.left() + tick * model.zoom_x.max(1e-6) - model.scroll_x.max(0.0)
}

/// 绘制标尺（`rect` 由宿主按布局给出，宽度即走带区宽度）。
pub fn show_ruler(ui: &egui::Ui, rect: Rect, model: &ArrangementRulerModel) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 0, theme::roll_ruler_bg());
    // 底部 1px 边框（生产画在 `height - 0.5`，即最后一行像素）。
    painter.rect_filled(
        Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 1.0), rect.max),
        0,
        theme::border(),
    );

    let zoom = model.zoom_x.max(1e-6);
    let scroll = model.scroll_x.max(0.0);
    let start_tick = scroll / zoom;
    let end_tick = (scroll + rect.width()) / zoom;

    let mut last_label_x = f32::NEG_INFINITY;
    for (tick, number) in measures::measure_marks(
        &model.time_signatures,
        model.ppq as f32,
        start_tick,
        end_tick,
    ) {
        let x = rect.left() + tick * zoom - scroll;
        if x < rect.left() - 0.5 || x > rect.right() {
            continue;
        }
        painter.rect_filled(
            Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(1.0, rect.height())),
            0,
            theme::border(),
        );
        if x - last_label_x < MIN_LABEL_SPACING {
            continue;
        }
        let label_x = x + LABEL_PAD;
        if label_x + 12.0 > rect.right() {
            continue;
        }
        painter.text(
            egui::pos2(label_x, rect.top() + LABEL_PAD),
            Align2::LEFT_TOP,
            number.to_string(),
            FontId::proportional(LABEL_SIZE),
            theme::text(),
        );
        last_label_x = x;
    }
}

/// 标尺命中：老式 Canvas 语义——按下与拖动都设光标，滚轮缩放。
fn ruler_input(
    ui: &egui::Ui,
    rect: Rect,
    model: &ArrangementRulerModel,
) -> Option<ArrangementAction> {
    let (pointer, down, pressed) = ui.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.primary_down(),
            i.pointer.any_pressed() && i.pointer.primary_pressed(),
        )
    });
    let zoom = model.zoom_x.max(1e-6);

    // 滚轮缩放：只在指针位于标尺内时生效（与 iced 相同的位置校验）。
    if let Some(pos) = pointer
        && rect.contains(pos)
        && let Some(factor) = zoom_factor(ui)
    {
        let local = (pos.x - rect.left()).clamp(0.0, rect.width());
        return Some(ArrangementAction::ZoomX {
            zoom: zoom * factor,
            fixed_ratio: if rect.width() > 0.0 {
                local / rect.width()
            } else {
                0.5
            },
        });
    }

    // 按下 / 拖动设光标。拖拽状态记在 ctx，避免移出标尺就断；
    // tick 未变化时不重复上报（按住时否则每帧一条）。
    let drag_id = egui::Id::unique("lumino.arrangement.ruler-drag");
    let tick_id = egui::Id::unique("lumino.arrangement.ruler-tick");
    let dragging = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(drag_id))
        .unwrap_or(false);

    let started = pressed && pointer.is_some_and(|pos| rect.contains(pos));
    if started {
        ui.ctx().data_mut(|d| d.insert_temp(drag_id, true));
    } else if !down {
        ui.ctx().data_mut(|d| d.remove::<bool>(drag_id));
        return None;
    }

    if !(dragging || started) {
        return None;
    }
    let pos = pointer?;
    let tick = tick_at(rect, model, pos.x);
    let last = ui.ctx().data(|d| d.get_temp::<f64>(tick_id));
    if last == Some(tick) {
        return None;
    }
    ui.ctx().data_mut(|d| d.insert_temp(tick_id, tick));
    Some(ArrangementAction::CursorSet(tick))
}

/// 屏幕 x → tick（负值钳 0，越界钳 `total_ticks`）。
fn tick_at(rect: Rect, model: &ArrangementRulerModel, x: f32) -> f64 {
    let local = (x - rect.left()).max(0.0) as f64;
    let zoom = model.zoom_x.max(1e-6) as f64;
    let scroll = model.scroll_x.max(0.0) as f64;
    ((local + scroll) / zoom)
        .max(0.0)
        .min(model.total_ticks.max(1) as f64)
}

/// 滚轮 → 缩放因子（与 iced `zoom_factor_from_delta` 同口径）。
///
/// 直接用原始 `Event::MouseWheel` 的 `unit`：Line 一个刻度 = 1 行，
/// Point 除以 50 换算成行（与 iced 的 `PIXEL_TO_LINE_SCALE` 一致），
/// 再钳到 ±1 行、每行 10%。
fn zoom_factor(ui: &egui::Ui) -> Option<f32> {
    const WHEEL_STEP: f32 = 0.1;
    const PIXEL_TO_LINE: f32 = 50.0;
    let mut lines = 0.0_f32;
    ui.input(|i| {
        for event in &i.raw.events {
            if let egui::Event::MouseWheel { unit, delta, .. } = event {
                lines += match unit {
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Point => delta.y / PIXEL_TO_LINE,
                    egui::MouseWheelUnit::Page => delta.y * 10.0,
                };
            }
        }
    });
    let step = lines.clamp(-1.0, 1.0);
    if step.abs() < f32::EPSILON {
        None
    } else {
        Some(1.0 + step * WHEEL_STEP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ArrangementRulerModel {
        ArrangementRulerModel {
            scroll_x: 200.0,
            zoom_x: 0.5,
            ppq: 960,
            left_inset: 0.0,
            top_inset: 0.0,
            time_signatures: vec![(0, 4, 4)],
            total_ticks: 100_000,
        }
    }

    fn rect() -> Rect {
        Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(800.0, RULER_HEIGHT))
    }

    #[test]
    fn tick_at_subtracts_scroll_and_clamps() {
        let m = model();
        let r = rect();
        // 指针在标尺左缘：local 0 → (0 + 200) / 0.5 = 400 tick。
        assert!((tick_at(r, &m, 100.0) - 400.0).abs() < 1e-6);
        // 右移 100 像素：local 100 → (100 + 200) / 0.5 = 600 tick。
        assert!((tick_at(r, &m, 200.0) - 600.0).abs() < 1e-6);
        // 左越界钳 0，右越界钳 total_ticks。
        assert_eq!(tick_at(r, &m, -50.0), 400.0);
        assert_eq!(tick_at(r, &m, 100_000.0), 100_000.0);
    }

    #[test]
    fn tick_to_x_inverts_tick_at() {
        let m = model();
        let r = rect();
        let x = tick_to_x(r, &m, 1000.0);
        assert!((tick_at(r, &m, x) - 1000.0).abs() < 1e-3);
    }
}
