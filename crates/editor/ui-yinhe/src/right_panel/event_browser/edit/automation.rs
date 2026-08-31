//! Automation 编辑 popup — 对应 yinhe `right_panel/event_browser/edit/automation.rs:299`
//!
//! 覆盖 CC/PB/RPN/NRPN/Tempo 的 value / tick / shape 三类编辑，
//! yinhe 原以 `egui::Area + DragValue + LaneUndoGuard` 实现；
//! iced 桩以 `column + number_popup_view / position_popup_view` 占位，
//! 保留 `AutoCtx` 寻址与 `Closed → apply + push_undo` 语义注释。

use iced_widget::{column, text};

use lumino_ui_core::{Element, window::Window};

use super::{number_popup_view, position_popup_view};

/// Automation 编辑上下文（对齐 yinhe `AutoCtx`）
///
/// 打包 lane 寻址所需的 3 字段，避免 popup 视图超过 7 参数。
#[derive(Debug, Clone)]
pub struct AutoCtx {
    pub track_idx: u16,
    pub lane_idx: usize,
    pub target_name: String,
    pub max_value: f32,
}

/// Automation 编辑聚合视图（value / tick / shape 三选一，占位）
///
/// `kind` 决定显示哪类 popup；实际路由由 `EditRequest::Auto*` 驱动。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationEditKind {
    Value,
    Tick,
    TickPosition,
    Shape,
}

pub fn view<'a>(
    window: &'a Window,
    ctx: &'a AutoCtx,
    kind: AutomationEditKind,
    tick: u32,
    value: f32,
) -> Element<'a> {
    match kind {
        AutomationEditKind::Value => number_popup_view(
            window,
            format!("Edit {} value", ctx.target_name),
            value as f64,
            (0.0, ctx.max_value as f64),
        ),
        AutomationEditKind::Tick => number_popup_view(
            window,
            format!("Edit {} tick", ctx.target_name),
            tick as f64,
            (0.0, u32::MAX as f64),
        ),
        AutomationEditKind::TickPosition => position_popup_view(
            window,
            format!("Edit {} position", ctx.target_name),
            tick,
            1,
            0,
        ),
        AutomationEditKind::Shape => shape_popup_view(window, ctx),
    }
}

fn shape_popup_view<'a>(_window: &'a Window, ctx: &'a AutoCtx) -> Element<'a> {
    column![
        text(format!("Edit {} shape", ctx.target_name)).size(11),
        text("Step / Linear / Curve (X1 Y1 X2 Y2)").size(10),
        text("Discrete: ☐  (Step vs Curve)").size(11),
        text("X1: 0.00 [0..0.25]  Y1: 0.00 [-0.5..0.5]").size(10),
        text("X2: 0.00 [-0.25..0] Y2: 0.00 [-0.5..0.5]").size(10),
    ]
    .spacing(4)
    .padding([8, 8])
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn automation_popup_variants() {
        let window = Window::new("Tokyo Night Storm");
        let ctx = AutoCtx {
            track_idx: 0,
            lane_idx: 0,
            target_name: "CC 7".to_string(),
            max_value: 127.0,
        };
        let _ = view(&window, &ctx, AutomationEditKind::Value, 0, 64.0);
        let _ = view(&window, &ctx, AutomationEditKind::Tick, 480, 64.0);
        let _ = view(&window, &ctx, AutomationEditKind::Shape, 0, 64.0);
    }
}
