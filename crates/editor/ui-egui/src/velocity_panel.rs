//! 力度 / CC / Tempo 面板（egui 版骨架 + 工具栏）。
//!
//! 数据曲线（力度柱 / CC 折线 / Tempo 折线）由 `lumino-gfx` 直接画进离屏纹理，
//! 本模块只补 egui 侧被底层纹理覆盖的部分：
//! - 工具栏（模式按钮 / CC 下拉 / 信息文字）；
//! - 网格线（小节 / 拍 / 半拍）与刻度标签；
//! - 顶部 resize 手柄的视觉（拖拽命中仍在 iced Canvas，保持原行为）。
//!
//! 面板背景不铺色：gfx 的清屏色已是 `background.base`，铺色会盖掉 gfx 数据曲线。

use egui::{Align2, CornerRadius, FontId, Margin, Pos2, Rect, Stroke, StrokeKind, Vec2};
use lumino_message::CcOption;

use crate::theme;

mod draw;
mod input;
mod types;

pub use types::{TempoPointModel, VelocityPointModel, VelocityView};

/// 面板工具栏高度（与 iced `TOOLBAR_HEIGHT` 一致）。
pub const TOOLBAR_HEIGHT: f32 = 28.0;
/// 顶部 resize 手柄高度。
const RESIZE_HANDLE: f32 = 5.0;

/// 垂直网格线强度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStrength {
    /// 小节线。
    Measure,
    /// 拍线。
    Beat,
    /// 半拍线。
    HalfBeat,
}

/// 力度面板模型（窗口逻辑坐标）。
#[derive(Debug, Clone, PartialEq)]
pub struct VelocityPanelModel {
    /// 整块面板。
    pub panel: Rect,
    /// 工具栏。
    pub toolbar: Rect,
    /// 数据区（网格 / 曲线）。
    pub canvas: Rect,
    /// 模式按钮文案（力度 / 速度 / CC / Bend）。
    pub mode_label: String,
    /// 模式按钮是否用主色（力度 / 速度）。
    pub mode_primary: bool,
    /// 是否显示 CC 下拉。
    pub cc_mode: bool,
    /// 当前 CC 选中项文案。
    pub cc_selected: String,
    /// 右侧信息文字。
    pub info: String,
    /// 水平网格线（canvas 局部 y, 标签）。
    pub hlines: Vec<(f32, String)>,
    /// 垂直网格线（canvas 局部 x, 强度）。
    pub vlines: Vec<(f32, LineStrength)>,
    /// 力度模式 + 曲线工具：egui 接管力度编辑。
    pub velocity_editing: bool,
    /// 力度点快照。
    pub velocity_points: Vec<VelocityPointModel>,
    /// 是否存在选中集（曲线绘制只影响选中音符）。
    pub has_selection: bool,
    /// 选中的音符索引。
    pub selected_notes: Vec<usize>,
    /// 时间轴视图参数。
    pub view: VelocityView,
    /// Tempo 模式（决定是否绘制折线图，与工具无关）。
    pub tempo_mode: bool,
    /// Tempo 模式：egui 接管 Tempo 编辑。
    pub tempo_editing: bool,
    /// Tempo 控制点快照。
    pub tempo_points: Vec<TempoPointModel>,
    /// Tempo BPM 绘制上限。
    pub tempo_max_bpm: f64,
    /// Tempo 折线宽度。
    pub tempo_line_width: f32,
    /// 当前是否橡皮工具（点击删除 Tempo 点）。
    pub eraser_tool: bool,
}

/// 本帧动作。
#[derive(Debug, Clone, PartialEq)]
pub enum VelocityPanelAction {
    /// 切换模式（力度 → 速度 → CC → Bend）。
    ToggleMode,
    /// 选择 CC / Bend。
    CcSelected(CcOption),
    /// 力度拖拽开始（note_index, 当前力度）。
    VelocityDragStart(usize, u8),
    /// 力度拖拽移动（note_index, 新力度）。
    VelocityDragMove(usize, u8),
    /// 力度拖拽结束。
    VelocityDragEnd,
    /// 力度曲线绘制开始。
    VelocityCurveStart,
    /// 力度曲线批量更新。
    VelocityCurvePaint(Vec<(usize, u8)>),
    /// 力度曲线绘制结束。
    VelocityCurveEnd,
    /// Tempo 拖拽开始（控制点索引）。
    TempoDragStart(usize),
    /// Tempo 拖拽移动（索引, BPM）。
    TempoDragMove(usize, f64),
    /// Tempo 拖拽结束。
    TempoDragEnd,
    /// Tempo 新增控制点（tick, BPM）。
    TempoAdd(f32, f64),
    /// Tempo 删除控制点。
    TempoDelete(usize),
    /// 双轴滚轮（时间轴 / 曲线）。
    Wheel { delta_x: f32, delta_y: f32 },
}

const ACTION_ID: &str = "lumino.velocity-panel.action";
const CC_OPEN_ID: &str = "lumino.velocity-panel.cc-open";

/// 记录本帧动作（一帧一条）。
pub fn push_action(ctx: &egui::Context, action: VelocityPanelAction) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<VelocityPanelAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<VelocityPanelAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 绘制面板骨架与工具栏。
///
/// 返回 `(动作, 需要从 iced 手里接管的矩形)`：工具栏与 CC 弹层必须占用，
/// 数据区的点击仍交给 iced Canvas（力度/CC 编辑逻辑不在本轮迁移范围）。
pub fn show(ui: &egui::Ui, model: &VelocityPanelModel) -> (Option<VelocityPanelAction>, Vec<Rect>) {
    let painter = ui.painter();
    let mut blocked = vec![model.toolbar];

    // 工具栏底色 + 底边线。
    painter.rect_filled(model.toolbar, CornerRadius::ZERO, theme::toolbar_bg());
    painter.line_segment(
        [
            Pos2::new(model.toolbar.left(), model.toolbar.bottom()),
            Pos2::new(model.toolbar.right(), model.toolbar.bottom()),
        ],
        Stroke::new(1.0, theme::border()),
    );

    // 模式按钮。
    let mode_rect = Rect::from_min_size(
        Pos2::new(model.toolbar.left() + 8.0, model.toolbar.top() + 4.0),
        Vec2::new(draw::text_width(ui, &model.mode_label, 12.0) + 16.0, 20.0),
    );
    let mode_fill = if model.mode_primary {
        theme::accent()
    } else {
        theme::base_bg()
    };
    let mode_text = if model.mode_primary {
        theme::on_accent()
    } else {
        theme::text()
    };
    painter.rect(
        mode_rect,
        CornerRadius::same(3),
        mode_fill,
        Stroke::NONE,
        StrokeKind::Inside,
    );
    painter.text(
        mode_rect.center(),
        Align2::CENTER_CENTER,
        &model.mode_label,
        FontId::proportional(12.0),
        mode_text,
    );
    let mode_resp = ui.interact(
        mode_rect,
        egui::Id::unique("lumino.velocity-panel.mode"),
        egui::Sense::click(),
    );
    let mut action = if mode_resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        Some(VelocityPanelAction::ToggleMode)
    } else {
        None
    };

    let mut cc_open = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(egui::Id::unique(CC_OPEN_ID)))
        .unwrap_or(false);

    // CC 选择器（自绘下拉：弹层矩形可精确占用，不会漏给 iced）。
    if model.cc_mode {
        let cc_rect = Rect::from_min_size(
            Pos2::new(mode_rect.right() + 8.0, model.toolbar.top() + 4.0),
            Vec2::new(170.0, 20.0),
        );
        painter.rect(
            cc_rect,
            CornerRadius::same(3),
            theme::base_bg(),
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        painter.text(
            Pos2::new(cc_rect.left() + 6.0, cc_rect.center().y),
            Align2::LEFT_CENTER,
            &model.cc_selected,
            FontId::proportional(12.0),
            theme::text(),
        );
        painter.text(
            Pos2::new(cc_rect.right() - 6.0, cc_rect.center().y),
            Align2::RIGHT_CENTER,
            "▾",
            FontId::proportional(10.0),
            theme::text_muted(),
        );
        let cc_resp = ui.interact(
            cc_rect,
            egui::Id::unique("lumino.velocity-panel.cc"),
            egui::Sense::click(),
        );
        if cc_resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            cc_open = !cc_open;
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(CC_OPEN_ID), cc_open));
        }
        blocked.push(cc_rect);
    }

    // 右侧信息。
    painter.text(
        Pos2::new(model.toolbar.right() - 8.0, model.toolbar.center().y),
        Align2::RIGHT_CENTER,
        &model.info,
        FontId::proportional(11.0),
        theme::text_muted().gamma_multiply(0.8),
    );

    // 顶部 resize 手柄视觉（命中仍在 iced Canvas）。
    let grab = Rect::from_center_size(
        Pos2::new(
            model.canvas.center().x,
            model.canvas.top() + RESIZE_HANDLE * 0.5,
        ),
        Vec2::new(40.0, 2.0),
    );
    painter.rect_filled(
        grab,
        CornerRadius::same(1),
        theme::text().gamma_multiply(0.35),
    );

    // 网格：垂直线（小节 / 拍 / 半拍）。
    for &(x, strength) in &model.vlines {
        let color = match strength {
            LineStrength::Measure => theme::text().gamma_multiply(0.45),
            LineStrength::Beat => theme::text().gamma_multiply(0.25),
            LineStrength::HalfBeat => theme::text().gamma_multiply(0.12),
        };
        let x = model.canvas.left() + x;
        painter.line_segment(
            [
                Pos2::new(x, model.canvas.top() + RESIZE_HANDLE),
                Pos2::new(x, model.canvas.bottom() - RESIZE_HANDLE),
            ],
            Stroke::new(1.0, color),
        );
    }
    // 水平网格 + 标签。
    for (y, label) in &model.hlines {
        let y = model.canvas.top() + *y;
        painter.line_segment(
            [
                Pos2::new(model.canvas.left() + 8.0, y),
                Pos2::new(model.canvas.right() - 8.0, y),
            ],
            Stroke::new(1.0, theme::text().gamma_multiply(0.18)),
        );
        painter.text(
            Pos2::new(model.canvas.left() + 4.0, y - 6.0),
            Align2::LEFT_TOP,
            label,
            FontId::proportional(9.0),
            theme::text().gamma_multiply(0.55),
        );
    }

    // Tempo 折线图（gfx 只画力度/CC 曲线，Tempo 图由 egui 补）。
    if model.tempo_mode && !model.tempo_points.is_empty() {
        draw::tempo_graph(painter, model);
    }

    // CC 弹层（滚动列表，129 项）。
    if model.cc_mode && cc_open {
        let cc_rect = Rect::from_min_size(
            Pos2::new(mode_rect.right() + 8.0, model.toolbar.top() + 4.0),
            Vec2::new(170.0, 20.0),
        );
        let popup_pos = Pos2::new(cc_rect.left(), cc_rect.bottom() + 2.0);
        let popup = egui::Area::new(egui::Id::unique("lumino.velocity-panel.cc-popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .fill(theme::panel_bg())
                    .stroke(Stroke::new(1.0, theme::border()))
                    .inner_margin(Margin::symmetric(2, 2))
                    .show(ui, |ui| {
                        ui.set_max_width(220.0);
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                let mut options = Vec::with_capacity(129);
                                options.push(CcOption::Bend);
                                for n in 0..=127u8 {
                                    options.push(CcOption::Cc(n));
                                }
                                let selected = model.cc_selected.as_str();
                                for option in options {
                                    let label = option.to_string();
                                    let is_selected = label == selected;
                                    if ui.selectable_label(is_selected, &label).clicked() {
                                        action = Some(VelocityPanelAction::CcSelected(option));
                                        cc_open = false;
                                    }
                                }
                            });
                    });
            });
        blocked.push(popup.response.rect);
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(CC_OPEN_ID), cc_open));
    } else if !model.cc_mode {
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(CC_OPEN_ID), false));
    }

    // 力度模式 + 曲线工具：egui 接管数据区编辑（iced Canvas 不再收这一段）。
    if model.velocity_editing {
        blocked.push(input::blocking_rect(model.canvas));
        if action.is_none() {
            action = input::handle(ui, model);
        }
    } else if model.tempo_editing {
        // Tempo 模式：egui 接管拖拽 / 新增 / 删除（resize 手柄仍归 iced）。
        blocked.push(input::blocking_rect(model.canvas));
        if action.is_none() {
            action = input::handle_tempo(ui, model);
        }
    }

    (action, blocked)
}
