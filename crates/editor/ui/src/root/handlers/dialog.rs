//! 对话框管理处理器 — 路由分发

mod audio_export;
mod batch_edit;
mod custom_precision;
mod load_confirm;
mod project_settings;
mod recover_track;
mod settings;
mod speed_change;
mod video_export;

use crate::message::Message;
use crate::root::Root;
use crate::root::handlers::MessageHandler;

/// 对话框消息处理器
pub struct DialogHandler;

impl DialogHandler {
    /// 创建一个对话框消息处理器
    pub fn new() -> Self {
        Self
    }
}

impl Default for DialogHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for DialogHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        match msg {
            Message::CustomPrecision(action) => self.handle_custom_precision(root, action),
            Message::LoadConfirm(action) => self.handle_load_confirm(root, action),
            Message::ProjectSettings(action) => self.handle_project_settings(root, action),
            Message::SettingsDialog(action) => self.handle_settings_dialog(root, action),
            Message::AudioExport(action) => self.handle_audio_export(root, action),
            Message::VideoExport(action) => self.handle_video_export(root, action),
            Message::SpeedChange(action) => self.handle_speed_change(root, action),
            Message::BatchEdit(action) => self.handle_batch_edit(root, action),
            Message::RecoverTrack(action) => self.handle_recover_track(root, action),
            other => Some(other),
        }
    }
}
