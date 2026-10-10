//! 音轨端口选择器（薄封装：内部 0..=15、UI 显示 1..=16）
//!
//! 公共网格/定位/关闭覆盖层见 [`super::numeric_picker`]。

use super::core::Sidebar;
use super::numeric_picker;
use crate::{Element, Message};

/// 端口选择事件（内部值 0..=15）。
fn selected(track_id: usize, port: u8) -> Message {
    lumino_ui_core::sidebar_event::Event::track_port_selected(track_id, port)
}

/// 端口选择器关闭事件。
fn closed(track_id: usize) -> Message {
    lumino_ui_core::sidebar_event::Event::track_port_picker_closed(track_id)
}

/// 构建端口选择器面板内容（当前端口高亮）。
pub fn panel(track_id: usize, current_port: u8) -> Element<'static> {
    numeric_picker::panel(track_id, current_port, Sidebar::PORT_CHOICES, selected)
}

/// 构建定位在触发音轨右侧的端口选择器覆盖层。
pub fn positioned_panel<'a>(track_id: usize, current_port: u8, top_y: f32) -> Element<'a> {
    numeric_picker::position(panel(track_id, current_port), top_y)
}

/// 点击外部区域关闭端口选择器。
pub fn background_close_overlay<'a>(track_id: usize) -> Element<'a> {
    numeric_picker::background_close_overlay(closed(track_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 端口数与合成层产品上限同源，且能整行排布。
    #[test]
    fn port_choices_align_with_multi_port_limit() {
        assert_eq!(
            Sidebar::PORT_CHOICES,
            lumino_midi_model::multi_port::MAX_PORTS,
            "UI 端口数必须与合成层产品上限同源（防分叉）"
        );
        assert_eq!(usize::from(Sidebar::PORT_CHOICES) % 4, 0, "应能整行排布");
    }

    #[test]
    fn port_picker_builders_return_elements() {
        let _ = panel(1, 0);
        let _ = positioned_panel(1, 3, 100.0);
        let _ = background_close_overlay(1);
    }
}
