//! 混音台：生产状态 → egui 模型 / 动作映射。
//!
//! 混音台是**独立 OS 窗口**（`DialogType::Mixer`）：主窗口只负责生成轨道 /
//! 电平快照（[`Root::mixer_snapshot`]），由 Runner 每帧推给对话框 Root；
//! 对话框产生的 [`lumino_ui_egui::mixer::MixerAction`] 再由 Runner 取回并
//! 映射成本文件的 [`Root::mixer_action_message`]。

use lumino_ui_egui::mixer::{MixerDialogModel, MixerModel, MixerStrip};

use crate::root::Root;

impl Root {
    /// 主窗口本帧的混音台快照（`None` = 混音台未打开）。
    ///
    /// 不含几何：对话框自己按客户端尺寸布局。`scroll_x` 恒为 0——横向滚动
    /// 是对话框本地状态（`MixerDialogState::scroll_x`），不跟随主窗口。
    pub(crate) fn mixer_snapshot(&self) -> Option<MixerModel> {
        if !self.mixer_panel.open {
            return None;
        }

        let frame = self.playback.manager.as_ref().and_then(|m| m.last_frame());
        let strips = self
            .sidebar
            .tracks
            .iter()
            .map(|track| {
                let params = self.sidebar.mixer_strip(track.id);
                let meter = frame
                    .as_ref()
                    .map(|f| {
                        f.channel_levels
                            .get(track.channel as usize)
                            .copied()
                            .unwrap_or(0.0)
                    })
                    .unwrap_or(0.0);
                MixerStrip {
                    name: track.display_label.clone(),
                    volume: params.gain.clamp(0.0, 1.0),
                    pan: params.pan.clamp(-1.0, 1.0),
                    muted: track.is_muted,
                    soloed: track.is_soloed,
                    meter: meter.clamp(0.0, 1.0),
                    master: false,
                }
            })
            .collect::<Vec<_>>();

        let master = Some(MixerStrip {
            name: "Master".to_string(),
            volume: (self.mixer_panel.master_volume as f32 / 127.0).clamp(0.0, 1.0),
            pan: 0.0,
            muted: false,
            soloed: false,
            meter: frame
                .as_ref()
                .map(|f| f.master_level)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0),
            master: true,
        });

        Some(MixerModel {
            scroll_x: 0.0,
            strips,
            master,
        })
    }

    /// 对话框窗口本帧的整窗模型（标题栏 + 数据 + 本地滚动）。
    pub(super) fn mixer_dialog_model(&self) -> MixerDialogModel {
        let mut mixer = self.mixer_dialog.model.clone();
        mixer.scroll_x = self.mixer_dialog.scroll_x;
        MixerDialogModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            mixer,
        }
    }

    /// 混音台动作 → 生产消息（构造函数已直接返回 `Message`）。
    ///
    /// `Scrolled` 是对话框本地滚动，不产生主窗口消息。
    pub(crate) fn mixer_action_message(
        &self,
        action: &lumino_ui_egui::mixer::MixerAction,
    ) -> Option<crate::Message> {
        use crate::sidebar::Event as SidebarEvent;
        use lumino_ui_egui::mixer::MixerAction as A;

        let track_id = |index: usize| self.sidebar.tracks.get(index).map(|t| t.id);
        Some(match action {
            A::Gain { index, value } => {
                let gain =
                    crate::sidebar::volume_to_gain((value.clamp(0.0, 1.0) * 127.0).round() as u8);
                SidebarEvent::track_gain_changed(track_id(*index)?, gain)
            }
            A::Pan { index, value } => SidebarEvent::track_pan_changed(track_id(*index)?, *value),
            A::Mute(index) => SidebarEvent::track_mute_toggled(track_id(*index)?),
            A::Solo(index) => SidebarEvent::track_solo_toggled(track_id(*index)?),
            A::MasterVolume(value) => SidebarEvent::mixer_panel_master_volume_changed(
                (value.clamp(0.0, 1.0) * 127.0).round() as u8,
            ),
            A::Scrolled(_) => return None,
        })
    }
}
