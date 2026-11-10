//! 混音台：生产状态 → egui 模型。
//!
//! 面板矩形按**窗口真实尺寸**锚定（左下角、状态栏之上），不再依赖 iced 面板的
//! 探针矩形——探针测的是 iced 的「全屏外层容器」，不是可见面板本体。

use lumino_ui_egui::mixer::{MixerLayout, MixerModel, MixerStrip};

use crate::root::Root;

impl Root {
    /// 窗口逻辑宽度/高度由宿主每帧传入（`size_px / ppp`）。
    pub(crate) fn mixer_model(
        &self,
        window_w: f32,
        window_h: f32,
    ) -> Option<(MixerLayout, MixerModel)> {
        if !self.mixer_panel.open {
            return None;
        }
        // 底缘 = 窗口高 − 状态栏；左缘 = 面板偏移。
        let bottom = window_h - STATUSBAR_H;
        let left = self.mixer_panel.offset.0;
        if bottom <= 0.0 || window_w <= 0.0 {
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

        let model = MixerModel {
            maximized: self.mixer_panel.maximized,
            scroll_x: self.mixer_panel.scroll_x,
            strips,
            master,
        };
        let layout = MixerLayout::new(left, bottom, window_w, &model);
        Some((layout, model))
    }
}

/// 状态栏高度（与 `layout::STATUSBAR_H` 一致；此处避免跨 crate 依赖）。
const STATUSBAR_H: f32 = 20.0;

impl Root {
    /// 混音台动作 → 生产消息（构造函数已直接返回 `Message`）。
    pub(crate) fn mixer_action_message(
        &self,
        action: &lumino_ui_egui::mixer::MixerAction,
    ) -> Option<crate::Message> {
        use crate::sidebar::Event as SidebarEvent;
        use lumino_ui_egui::mixer::MixerAction as A;

        let track_id = |index: usize| self.sidebar.tracks.get(index).map(|t| t.id);
        Some(match action {
            A::DragStarted => SidebarEvent::mixer_panel_drag_started(),
            A::DragBy { dx, dy } => SidebarEvent::mixer_panel_drag_delta(*dx, *dy),
            A::DragEnded => SidebarEvent::mixer_panel_drag_ended(),
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
        })
    }
}
