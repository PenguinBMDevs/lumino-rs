//! 自动化面板小部件 — 对应 yinhe `automation_panel/widgets.rs`
//!
//! iced 桩：无 egui `Ui`，改为 `Program::State` + `canvas::Frame` 几何 + `Message` 分派。

use iced_core::{Color, Point, Rectangle, Size};

use lumino_note_core::AutomationTarget;
use lumino_ui_core::{Renderer, Theme};

use super::constants::AUTOMATION_TARGETS;
use super::types::{AutomationPanelView, Tool};

// ── 分割条 ─────────────────────────────────────────────────────────────

/// 分割条拖拽的增量应用（由 `Program::update` 的 `CursorMoved` 分支调用）。
///
/// `delta_y` 为鼠标 y 增量（像素），正值 = 向下拖动 → 面板变矮（与 yinhe `panel_height -= delta.y` 一致）。
pub fn apply_split_drag(panel: &mut AutomationPanelView, delta_y: f32) {
    panel.panel_height = (panel.panel_height - delta_y).clamp(
        super::constants::MIN_PANEL_HEIGHT,
        super::constants::MAX_PANEL_HEIGHT,
    );
    panel.dirty = true;
}

/// 分割条双击：重置为默认高度。
pub fn reset_split_height(panel: &mut AutomationPanelView) {
    panel.panel_height = super::constants::DEFAULT_PANEL_HEIGHT;
    panel.dirty = true;
}

// ── 目标选择器 ───────────────────────────────────────────────────────

/// 目标选择器条目（下拉菜单用）。
#[derive(Clone, Debug)]
pub struct TargetComboEntry {
    pub label: String,
    pub target: Option<AutomationTarget>,
    pub is_velocity: bool,
    pub selected: bool,
}

/// 生成目标选择器下拉的条目列表（供 iced `pick_list` / `menu` 渲染）。
#[must_use]
pub fn target_combo_entries(
    panel: &AutomationPanelView,
    _editing_is_conductor: bool,
) -> Vec<TargetComboEntry> {
    let mut out = Vec::new();
    // Velocity 模式条目
    out.push(TargetComboEntry {
        label: "Velocity".to_string(),
        target: None,
        is_velocity: true,
        selected: panel.show_velocity,
    });
    for t in AUTOMATION_TARGETS {
        let label = t.display_name();
        let selected = !panel.show_velocity && panel.selected_target == *t;
        out.push(TargetComboEntry {
            label,
            target: Some(t.clone()),
            is_velocity: false,
            selected,
        });
    }
    out
}

/// 应用下拉选择（`pick_list` 选中回调）。
pub fn apply_target_selection(panel: &mut AutomationPanelView, entry: &TargetComboEntry) {
    if entry.is_velocity {
        panel.show_velocity = true;
    } else if let Some(t) = &entry.target {
        panel.selected_target = t.clone();
        panel.show_velocity = false;
    }
    panel.dirty = true;
}

// ── 切换/增删按钮 ────────────────────────────────────────────────────

/// 面板显隐/增删按钮的状态（供 chrome 侧 toolbar 渲染）。
#[derive(Clone, Copy, Debug, Default)]
pub struct ToggleState {
    pub show_panels: bool,
    pub panel_count: usize,
}

impl ToggleState {
    pub fn toggle(&mut self) {
        self.show_panels = !self.show_panels;
        if self.show_panels && self.panel_count == 0 {
            self.panel_count = 1;
        }
    }
    pub fn add_panel(&mut self) {
        self.panel_count += 1;
    }
    pub fn remove_panel(&mut self) {
        self.panel_count = self.panel_count.saturating_sub(1);
    }
}

// ── 绘制 helpers（iced canvas 占位） ─────────────────────────────────

/// 在 `Frame` 中绘制分割条（1px 线 + 抓手指示）。
pub fn draw_split_handle(
    frame: &mut iced_widget::canvas::Frame<Renderer>,
    rect: Rectangle,
    _theme: &Theme,
) {
    let _ = (frame, rect);
}

/// 在 `Frame` 中绘制目标选择器按钮区域（图标占位）。
pub fn draw_target_combo(
    frame: &mut iced_widget::canvas::Frame<Renderer>,
    rect: Rectangle,
    _panel: &AutomationPanelView,
) {
    let _ = (frame, rect);
}

/// 绘制面板标题与值标签（顶部/中部/底部 + 目标名，与 yinhe `draw_value_labels` 对齐）。
pub fn draw_value_labels(
    frame: &mut iced_widget::canvas::Frame<Renderer>,
    _panel: &AutomationPanelView,
    _panel_rect: Rectangle,
    _combo_width: f32,
    _max_val: f32,
) {
    let _ = frame;
}

// ── 工具栏按钮几何 ───────────────────────────────────────────────────

/// 生成工具切换按钮的命中矩形（供 `hit_test` 使用）。
#[must_use]
pub fn tool_button_rect(origin: Point, index: usize) -> Rectangle {
    let x = origin.x + index as f32 * 28.0;
    Rectangle::new(Point::new(x, origin.y), Size::new(24.0, 24.0))
}

/// `Tool` 的显示名（状态栏/提示用）。
#[must_use]
pub fn tool_label(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "Select",
        Tool::SelectVertical => "Select V",
        Tool::Pencil => "Pencil",
        Tool::Curve => "Curve",
        Tool::Eraser => "Eraser",
    }
}

/// CC 颜色（由调用方注入主题，此处提供默认调色占位）。
#[must_use]
pub fn default_bar_color() -> Color {
    Color::from_rgba(0.2, 0.55, 1.0, 0.85)
}
