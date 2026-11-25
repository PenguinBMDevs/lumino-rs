//! 「端口音色库未生效」提示窗：生产状态 → egui 模型，动作 → `Message`。

use lumino_core::{tr, trf};
use lumino_ui_egui::chrome::PortSoundfontsUnsupportedModel;

use crate::Message;
use crate::root::Root;

impl Root {
    /// 提示窗模型（REND-005）。
    pub(super) fn port_soundfonts_unsupported_model(&self) -> PortSoundfontsUnsupportedModel {
        let state = &self.state.port_soundfonts_unsupported_dialog;
        PortSoundfontsUnsupportedModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("端口音色库未生效").to_string(),
            message: trf!(
                "当前后端「{backend}」不支持按端口音色库。",
                backend = state.backend()
            ),
            detail: trf!(
                "已保存 {ports} 个端口的映射；当前播放与导出将使用全局音色库（降级运行）。切换到 XSynth 后可恢复按端口音色库。",
                ports = state.ports()
            ),
            acknowledge: tr!("继续（使用全局音色库）").to_string(),
            open_settings: tr!("打开合成器设置").to_string(),
        }
    }

    /// egui 提示窗动作 → 生产 `Message`。
    pub(crate) fn port_soundfonts_unsupported_action_message(
        &self,
        action: &lumino_ui_egui::port_soundfonts_unsupported::PortSoundfontsUnsupportedAction,
    ) -> Option<Message> {
        use lumino_message::PortSoundfontsUnsupportedAction as Prod;
        use lumino_ui_egui::port_soundfonts_unsupported::PortSoundfontsUnsupportedAction as Action;
        Some(Message::PortSoundfontsUnsupported(match action {
            Action::Acknowledge => Prod::Acknowledge,
            Action::OpenSettings => Prod::OpenSettings,
        }))
    }
}
