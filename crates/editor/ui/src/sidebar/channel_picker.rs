//! 音轨通道选择器（薄封装：内部 0..=15、UI 显示 1..=16）
//!
//! 公共网格/定位/关闭覆盖层见 [`super::numeric_picker`]。

use super::core::Sidebar;
use super::numeric_picker;
use crate::{Element, Message};

/// 通道选择事件（内部值 0..=15）。
fn selected(track_id: usize, channel: u8) -> Message {
    lumino_ui_core::sidebar_event::Event::track_channel_selected(track_id, channel)
}

/// 通道选择器关闭事件。
fn closed(track_id: usize) -> Message {
    lumino_ui_core::sidebar_event::Event::track_channel_picker_closed(track_id)
}

/// 构建通道选择器面板内容（当前通道高亮）。
pub fn panel(track_id: usize, current_channel: u8) -> Element<'static> {
    numeric_picker::panel(
        track_id,
        current_channel,
        Sidebar::CHANNEL_CHOICES,
        selected,
    )
}

/// 构建定位在触发音轨右侧的通道选择器覆盖层。
pub fn positioned_panel<'a>(track_id: usize, current_channel: u8, top_y: f32) -> Element<'a> {
    numeric_picker::position(panel(track_id, current_channel), top_y)
}

/// 点击外部区域关闭通道选择器。
pub fn background_close_overlay<'a>(track_id: usize) -> Element<'a> {
    numeric_picker::background_close_overlay(closed(track_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 通道数与 midi-io 标准通道数同源，且能整行排布。
    #[test]
    fn channel_choices_match_midi_channel_count() {
        assert_eq!(
            Sidebar::CHANNEL_CHOICES,
            lumino_midi_io::MIDI_CHANNEL_COUNT,
            "UI 通道数必须与 MIDI 标准同源"
        );
        assert_eq!(usize::from(Sidebar::CHANNEL_CHOICES) % 4, 0, "应能整行排布");
    }

    #[test]
    fn channel_picker_builders_return_elements() {
        let _ = panel(1, 0);
        let _ = positioned_panel(1, 3, 100.0);
        let _ = background_close_overlay(1);
    }
}
