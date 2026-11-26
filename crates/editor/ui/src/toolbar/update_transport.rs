//! 走带、循环与录制事件的工具栏状态。

use super::*;

impl Toolbar {
    pub(super) fn update_transport(&mut self, event: Event) {
        match event {
            Event::Play => self.is_playing = true,
            Event::Pause => self.is_playing = false,
            Event::Stop => self.is_playing = false,
            // 跳转由 Root / Runner 处理；工具栏只保持按钮状态一致。
            Event::SkipBackward
            | Event::SkipForward
            | Event::SkipBackwardToStart
            | Event::SkipForwardToEnd => {}
            Event::ToggleLoop => {
                self.is_looping = !self.is_looping;
                tracing::debug!("工具栏: 循环播放切换为 {}", self.is_looping);
            }
            Event::Record => {
                self.is_recording = true;
                tracing::debug!("工具栏: 开始录制");
            }
            Event::RecordStop => {
                self.is_recording = false;
                tracing::debug!("工具栏: 停止录制");
            }
            _ => {}
        }
    }
}
