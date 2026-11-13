//! winit ↔ iced 的最小转换（脱离 `iced_winit` 依赖）。
//!
//! 只保留仍在使用的两条：
//! - 物理坐标 → 逻辑坐标（`CursorMoved` / `Touch`）；
//! - iced 的 `mouse::Interaction` → winit 光标图标（画布光标反馈）。
//!
//! 映射表与 iced 0.14 `winit/src/conversion.rs` 逐项一致，避免图标语义漂移。

use iced_core::mouse::Interaction;

/// 物理坐标 → 逻辑坐标（与 `iced_winit::conversion::cursor_position` 同公式）。
pub(crate) fn cursor_position(
    position: winit::dpi::PhysicalPosition<f64>,
    scale: f32,
) -> iced_core::Point {
    iced_core::Point::new(
        (position.x / f64::from(scale)) as f32,
        (position.y / f64::from(scale)) as f32,
    )
}

/// iced 鼠标交互 → winit 光标图标（`None` = 隐藏光标）。
pub(crate) fn mouse_interaction(interaction: Interaction) -> Option<winit::window::CursorIcon> {
    use winit::window::CursorIcon;

    let icon = match interaction {
        Interaction::Hidden => return None,
        Interaction::None | Interaction::Idle => CursorIcon::Default,
        Interaction::ContextMenu => CursorIcon::ContextMenu,
        Interaction::Help => CursorIcon::Help,
        Interaction::Pointer => CursorIcon::Pointer,
        Interaction::Progress => CursorIcon::Progress,
        Interaction::Wait => CursorIcon::Wait,
        Interaction::Cell => CursorIcon::Cell,
        Interaction::Crosshair => CursorIcon::Crosshair,
        Interaction::Text => CursorIcon::Text,
        Interaction::Alias => CursorIcon::Alias,
        Interaction::Copy => CursorIcon::Copy,
        Interaction::Move => CursorIcon::Move,
        Interaction::NoDrop => CursorIcon::NoDrop,
        Interaction::NotAllowed => CursorIcon::NotAllowed,
        Interaction::Grab => CursorIcon::Grab,
        Interaction::Grabbing => CursorIcon::Grabbing,
        Interaction::ResizingHorizontally => CursorIcon::EwResize,
        Interaction::ResizingVertically => CursorIcon::NsResize,
        Interaction::ResizingDiagonallyUp => CursorIcon::NeswResize,
        Interaction::ResizingDiagonallyDown => CursorIcon::NwseResize,
        Interaction::ResizingColumn => CursorIcon::ColResize,
        Interaction::ResizingRow => CursorIcon::RowResize,
        Interaction::AllScroll => CursorIcon::AllScroll,
        Interaction::ZoomIn => CursorIcon::ZoomIn,
        Interaction::ZoomOut => CursorIcon::ZoomOut,
    };

    Some(icon)
}
