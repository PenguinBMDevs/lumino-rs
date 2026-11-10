//! 工程走带视图：生产状态 → egui 模型（标尺 + 轨道列）。
//!
//! 只在走带路由激活时产出；几何量与 iced `view_arrangement` 同源
//! （`arrangement_view.viewport` + `editor_state` + `sidebar.tracks`）。

use lumino_ui_egui::chrome::{
    ArrangementModel, ArrangementRulerModel, ArrangementTrackListModel, ArrangementTrackRow,
};

use crate::root::Root;

/// iced `Color` → egui `Color32`（sRGB 直通）。
fn color32(color: iced_core::Color) -> egui::Color32 {
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgb(channel(color.r), channel(color.g), channel(color.b))
}

impl Root {
    /// 走带视图模型；`None` = 当前不在走带路由。
    pub(crate) fn arrangement_model(&self) -> Option<ArrangementModel> {
        if !self.sidebar.is_arrangement_route() {
            return None;
        }
        let vp = &self.arrangement_view.viewport;
        // 左缘 = 左侧栏（48 + 面板宽）；顶缘 = 标题栏 + **实际**工具栏高度。
        let left_inset = 48.0
            + if self.sidebar.panel_visible {
                self.sidebar.panel_width
            } else {
                0.0
            };
        let top_inset = if self.settings.synth.use_native_titlebar {
            0.0
        } else {
            lumino_ui_egui::layout::TITLEBAR_H
        } + self.toolbar.height;

        let tracks = self
            .sidebar
            .tracks
            .iter()
            .map(|track| ArrangementTrackRow {
                id: track.id,
                name: track.name.clone(),
                label: track.display_label.clone(),
                channel: track.channel,
                conductor: track.is_conductor,
                selected: track.id == self.sidebar.selected_track,
                muted: track.is_muted,
                soloed: track.is_soloed,
                color: track.color.map(color32),
            })
            .collect();

        let reorder = self
            .sidebar
            .track_reorder
            .as_ref()
            .filter(|state| state.active);

        Some(ArrangementModel {
            ruler: ArrangementRulerModel {
                scroll_x: vp.scroll_x,
                zoom_x: vp.zoom_x,
                ppq: self.editor.editor_state.view.ppq,
                left_inset,
                top_inset,
                time_signatures: self.editor.editor_state.data.time_signatures.clone(),
            },
            track_list: ArrangementTrackListModel {
                tracks,
                track_height: vp.track_height * vp.zoom_y,
                scroll_y: vp.scroll_y,
                drag_active: reorder.is_some(),
                drag_track: reorder.map(|state| state.track_id),
                drop_index: reorder.and_then(|state| state.hover_index),
            },
        })
    }
}
