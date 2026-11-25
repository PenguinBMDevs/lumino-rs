//! 端口音色库对话框：生产状态 → egui 模型，以及动作 → `Message` 映射。
//!
//! 单独成文件把 `dialogs.rs` 保持在 400 行以内。

use lumino_core::{tr, trf};
use lumino_ui_egui::chrome::{PortSoundfontsModel, PortSoundfontsRowModel};

use crate::Message;
use crate::root::Root;

impl Root {
    /// 端口音色库对话框模型（REND-005）。
    pub(super) fn port_soundfonts_model(&self) -> PortSoundfontsModel {
        let rows = self
            .state
            .port_soundfonts_dialog
            .rows()
            .iter()
            .map(|row| {
                let path = row.paths.first().cloned().unwrap_or_default();
                let extra = (row.paths.len() > 1).then(|| {
                    trf!(
                        "另有 {n} 个回退库（当前 UI 只显示首个）",
                        n = row.paths.len() - 1
                    )
                });
                PortSoundfontsRowModel {
                    port: row.port,
                    label: if row.port == 0 {
                        tr!("端口 0（全局，默认，可覆盖）").to_string()
                    } else {
                        trf!("端口 {port}", port = row.port)
                    },
                    path,
                    path_hint: if row.port == 0 {
                        tr!("未覆盖：使用全局音色库").to_string()
                    } else {
                        tr!("未配置：回退全局音色库").to_string()
                    },
                    extra,
                    browse: tr!("选择…").to_string(),
                    clear: tr!("清除").to_string(),
                }
            })
            .collect();

        PortSoundfontsModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("端口音色库").to_string(),
            hint: tr!(
                "为每个 MIDI 端口绑定音色库；未配置的端口使用全局音色库，端口 0 可覆盖全局。播放中保存的修改将在停播后生效。"
            )
            .to_string(),
            rows,
            ok: tr!("确定").to_string(),
            cancel: tr!("取消").to_string(),
        }
    }

    /// egui 端口音色库动作 → 生产 `Message`。
    pub(crate) fn port_soundfonts_action_message(
        &self,
        action: &lumino_ui_egui::port_soundfonts::PortSoundfontsAction,
    ) -> Option<Message> {
        use lumino_message::PortSoundfontsAction as Prod;
        use lumino_ui_egui::port_soundfonts::PortSoundfontsAction as Action;
        Some(Message::PortSoundfonts(match action {
            Action::Browse(port) => Prod::Browse(*port),
            Action::Clear(port) => Prod::Clear(*port),
            Action::Confirm => Prod::Confirm,
            Action::Cancel => Prod::Cancel,
        }))
    }
}
