//! 工具栏：走带、工具、仪表盘、右侧显示开关。
//!
//! 结构对齐生产 上一代 `toolbar_view`：
//! - 背景 = `background.weakest`（PANEL_BG）；
//! - 每个分组是一个 50px 高、圆角 4 的 `background.weak`（TOOLBAR_BG）容器；
//! - 分组间距：录制 4 / 播放 8 / 循环 8 / 撤销 16 / 仪表盘 16 / 工具 0；
//! - 右侧 OnionSkin / AutoScroll / Collaboration / More 直接贴在工具栏底色上。
//!
//! 分组绘制拆到三个子模块（单文件行数纪律）：
//! - [`controls`]：录制 / 走带 / 循环 / 撤销重做 / 检测仪表盘；
//! - [`tools`]：工具选择区 + 精度下拉；
//! - [`right_side`]：洋葱皮 / 自动滚动 / 协作 / 「更多」。
//!
//! [`precision`] 单独成模块：它在**帧外**也被调用（生产折叠预算），不能依赖 `Ui`。

mod controls;
/// 精度选择器宽度度量：绘制与折叠预算共用的唯一来源（生产折叠在帧外决策）。
pub mod precision;
mod right_side;
mod tools;

use egui::{Align, CornerRadius, Layout, Rect, Stroke, Vec2};
use lumino_core::Tool;

use super::toolbar_popup::{self, ToolbarAction};
use super::{ChromeState, ToolbarModel};
use crate::icons::Icon;
use crate::layout;
use crate::theme;

/// 草稿场景入口：使用默认状态（与 上一代 基线一致）。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    show_model(ui, &ToolbarModel::default(), &mut state.icons);
}

/// 渲染工具栏。
pub fn show_model(ui: &mut egui::Ui, model: &ToolbarModel, icons: &mut crate::icons::IconCache) {
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0, theme::panel_bg());
    if std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0") {
        tracing::warn!(?full, "DEBUG toolbar band");
    }

    let content = Rect::from_min_size(
        full.min,
        Vec2::new(
            full.width(),
            (full.height() - layout::TOOLBAR_RESIZE_H).max(0.0),
        ),
    );
    // 高度被用户拖高时，分组整体垂直居中（默认 72 时等于 `TOOLBAR_PAD_Y`）。
    let y = content.top() + ((content.height() - layout::TOOLBAR_GROUP_H) / 2.0).max(0.0);
    let h = layout::TOOLBAR_GROUP_H;
    let mut x = content.left() + layout::TOOLBAR_PAD_X;

    // Record
    if model.groups.record {
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::RECORD_GROUP_W, h)),
            |ui| controls::record(ui, model.recording, model.tips.record),
        );
        x += layout::RECORD_GROUP_W + 4.0;
    }

    // Playback
    if model.groups.playback {
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::PLAYBACK_GROUP_W, h)),
            |ui| controls::playback(ui, icons, model.playing, &model.tips),
        );
        x += layout::PLAYBACK_GROUP_W + 8.0;
    }

    // Loop
    if model.groups.looping {
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::LOOP_GROUP_W, h)),
            |ui| controls::loop_button(ui, icons, model.looping, model.tips.loop_toggle),
        );
        x += layout::LOOP_GROUP_W + 8.0;
    }

    // Undo / Redo / Clock
    if model.groups.undo_redo {
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::UNDO_REDO_GROUP_W, h)),
            |ui| controls::undo_redo(ui, icons, &model.tips),
        );
        x += layout::UNDO_REDO_GROUP_W + 16.0;
    }

    // Dashboard
    if model.groups.dashboard {
        group(
            ui,
            Rect::from_min_size(egui::pos2(x, y), Vec2::new(layout::DASHBOARD_GROUP_W, h)),
            |ui| controls::dashboard(ui, model),
        );
        x += layout::DASHBOARD_GROUP_W + 16.0;
    } else {
        // 分组被折叠时不画输入框：清掉两个字段的编辑会话标记，否则窗口变宽后
        // 重新展开时输入框会拿不到键盘（见 `metric_focus_once_id`）。
        for field in [DashboardField::Bpm, DashboardField::Ppq] {
            reset_metric_edit_session(ui.ctx(), field);
        }
    }

    // Tools（宽度由条目尺寸累加，对应 上一代 的 `Length::Shrink`）
    if model.groups.tools {
        let visible = visible_tool_count(model);
        // 精度选择器宽度随吸附文本变化：与生产折叠预算同一函数，总宽同步让位。
        let precision_width = precision::selector_width(&model.precision_label, &model.snap);
        group(
            ui,
            Rect::from_min_size(
                egui::pos2(x, y),
                Vec2::new(tools::width(visible, precision_width), h),
            ),
            |ui| tools::show(ui, icons, model),
        );
    }

    right_side::show(ui, content, icons, model);
    resize_handle(ui, full);
}

/// 底部 6px 高度拖拽手柄（上一代 `render_resize_handle` 的等价实现）。
///
/// 宽度跟随仍由宿主处理（`input.rs` 在 `is_toolbar_resizing` 时按光标 y 调
/// `Toolbar::update_resize_position`），这里只发成对的起止信号与拖拽位置。
fn resize_handle(ui: &egui::Ui, full: Rect) {
    let rect = Rect::from_min_max(
        egui::pos2(
            full.left(),
            (full.bottom() - layout::TOOLBAR_RESIZE_H).max(full.top()),
        ),
        full.max,
    );
    if rect.height() <= 0.0 {
        return;
    }
    let response = ui.interact(
        rect,
        egui::Id::unique("lumino.toolbar.resize"),
        egui::Sense::click_and_drag(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
    let dragging_id = egui::Id::unique("lumino.toolbar.resizing");
    let started = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(dragging_id))
        .unwrap_or(false);
    let down = response.is_pointer_button_down_on();
    if down && !started {
        ui.ctx().data_mut(|d| d.insert_temp(dragging_id, true));
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::ResizeStarted);
    } else if down && let Some(pos) = ui.ctx().pointer_latest_pos() {
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::ResizeDragged(pos.y));
    } else if !down && started {
        ui.ctx().data_mut(|d| d.insert_temp(dragging_id, false));
        toolbar_popup::push_action(ui.ctx(), ToolbarAction::ResizeEnded);
    }
    // 手柄由 egui 独占（上一代 侧已无同位置控件）。
    super::menu::extend_blocked(ui.ctx(), [rect]);
}

/// 工具区可见条目数（`ALL` 前缀；模型默认 `usize::MAX` = 全部）。
fn visible_tool_count(model: &ToolbarModel) -> usize {
    model.visible_tools.min(tools::TOOLS.len())
}

/// 分组容器：先铺底色，再在其中布局。
pub(super) fn group<R>(ui: &mut egui::Ui, rect: Rect, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(layout::TOOLBAR_GROUP_RADIUS),
        theme::toolbar_bg(),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing = Vec2::ZERO;
    add(&mut child)
}

/// 固定矩形内的居中图标按钮。
pub(super) fn icon_at(
    ui: &mut egui::Ui,
    cache: &mut crate::icons::IconCache,
    icon: Icon,
    rect: Rect,
    icon_size: f32,
    selected: bool,
) -> egui::Response {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::centered_and_justified(egui::Direction::TopDown)),
    );
    let image = cache.default_image(child.ctx(), icon, icon_size);
    let button = if selected {
        egui::Button::image(image)
            .fill(theme::selected_bg())
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(4))
    } else {
        egui::Button::image(image).frame(false)
    };
    child.add(button)
}

/// 当前工具是否对应工具区里的第 N 个选择器（0..=3）。
pub(super) fn tool_selected(model: &ToolbarModel, index: usize) -> bool {
    matches!(
        (index, model.tool),
        (0, Tool::Pointer) | (1, Tool::PointerYSelect) | (2, Tool::Pencil) | (3, Tool::Eraser)
    )
}

// ── 仪表盘字段编辑的失焦信号 ──────────────────────────────────────

/// 仪表盘可编辑字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardField {
    /// 速度（BPM，上行）。
    Bpm,
    /// 每四分音符脉冲数（PPQ，下行）。
    Ppq,
}

impl DashboardField {
    /// 稳定的 egui id 片段。
    fn key(self) -> &'static str {
        match self {
            Self::Bpm => "bpm",
            Self::Ppq => "ppq",
        }
    }

    /// 该字段当前是否处于编辑态。
    pub(super) fn editing(self, model: &ToolbarModel) -> bool {
        match self {
            Self::Bpm => model.bpm_editing,
            Self::Ppq => model.ppq_editing,
        }
    }

    /// 编辑缓冲文本。
    pub(super) fn buffer(self, model: &ToolbarModel) -> &str {
        match self {
            Self::Bpm => &model.bpm_buffer,
            Self::Ppq => &model.ppq_buffer,
        }
    }

    /// 校验错误文本。
    pub(super) fn error(self, model: &ToolbarModel) -> Option<&str> {
        match self {
            Self::Bpm => model.bpm_error.as_deref(),
            Self::Ppq => model.ppq_error.as_deref(),
        }
    }
}

/// 字段编辑框失焦的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricBlur {
    /// 回车 / Tab / 点击其它位置：提交当前输入。
    Commit,
    /// Esc：放弃本次编辑，不写入。
    Cancel,
}

/// 一次失焦信号：哪个字段 + 提交/取消。
///
/// 不复用 [`toolbar_popup::ToolbarAction`]：那条通道只有单槽，而「输入框失焦」
/// 通常恰好伴随一次按钮点击（点播放、点工具…），两者挤进同一槽必然吞掉一个。
/// 单独记一条信号，由宿主在分发任何控件动作**之前**先收口编辑。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashboardBlur {
    /// 失焦字段。
    pub field: DashboardField,
    /// 结局。
    pub outcome: MetricBlur,
}

/// 失焦信号的临时数据键（每帧至多一条）。
const DASHBOARD_BLUR_ID: &str = "lumino.toolbar.dashboard-blur";

/// 记录本帧仪表盘输入框失焦。
pub(super) fn push_dashboard_blur(ctx: &egui::Context, blur: DashboardBlur) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(DASHBOARD_BLUR_ID), Some(blur)));
}

/// 取走本帧仪表盘失焦信号（宿主每帧调用一次，取走即清）。
pub fn take_dashboard_blur(ctx: &egui::Context) -> Option<DashboardBlur> {
    ctx.data_mut(|d| d.remove_temp::<Option<DashboardBlur>>(egui::Id::unique(DASHBOARD_BLUR_ID)))
        .flatten()
}

/// 字段编辑会话「已抢过焦点」标记的 id。
///
/// 编辑结束时必须清除（见 `controls::dashboard` 的非编辑字段分支）：egui 的临时
/// 数据跨帧留存，不清的话第二个编辑会话不会 `request_focus`，输入框拿不到
/// 键盘——打字无反应，失焦提交也无从触发。
pub(super) fn metric_focus_once_id(field: DashboardField) -> egui::Id {
    egui::Id::unique(("lumino.toolbar.metric-focused", field.key()))
}

/// 本次编辑会话「失焦已上报」标记的 id。
///
/// egui 0.37 的 `Response::lost_focus` 会连续两帧为真（为同帧焦点转移留出
/// 补偿窗口），不去重会把同一次失焦提交两遍。
fn metric_blur_reported_id(field: DashboardField) -> egui::Id {
    egui::Id::unique(("lumino.toolbar.metric-blur-reported", field.key()))
}

/// 清除字段编辑会话的临时标记（抢焦点 / 失焦已上报）。
///
/// 字段退出编辑后调用（含仪表盘分组被折叠、输入框未绘制的情况），保证下一个
/// 编辑会话从干净状态开始。
pub(super) fn reset_metric_edit_session(ctx: &egui::Context, field: DashboardField) {
    ctx.data_mut(|d| {
        d.remove_temp::<bool>(metric_focus_once_id(field));
        d.remove_temp::<bool>(metric_blur_reported_id(field));
    });
}
