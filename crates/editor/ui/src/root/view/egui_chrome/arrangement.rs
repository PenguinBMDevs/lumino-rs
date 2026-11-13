//! 工程走带视图：生产状态 → egui 模型（标尺 + 轨道列）。
//!
//! 只在走带路由激活时产出；几何量与 旧版 `view_arrangement` 同源
//! （`arrangement_view.viewport` + `editor_state` + `sidebar.tracks`）。

use lumino_ui_egui::chrome::{
    ArrangementAction, ArrangementLaneModel, ArrangementModel, ArrangementRulerModel,
    ArrangementScrollbarsModel, ArrangementTrackListModel, ArrangementTrackRow,
};

use crate::root::Root;

/// 旧版 `Color` → egui `Color32`（sRGB 直通）。
fn color32(color: lumino_ui_core::geom::Color) -> egui::Color32 {
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

        // 两条滚动条：与 旧版 `view_arrangement` 同口径。
        const TRACK_HEIGHT: f32 = 48.0;
        let max_tick_val = vp
            .cached_max_tick_end
            .max(vp.total_ticks as f32)
            .max(960.0 * 4.0);
        let max_scroll_x = (max_tick_val * vp.zoom_x).max(vp.canvas_size.x);
        let max_scroll_y = self.sidebar.tracks.len() as f32 * TRACK_HEIGHT * vp.zoom_y;

        Some(ArrangementModel {
            scrollbars: Some(ArrangementScrollbarsModel {
                scroll_x: vp.scroll_x,
                max_scroll_x,
                zoom_x: vp.zoom_x,
                viewport_x: vp.canvas_size.x,
                scroll_y: vp.scroll_y,
                max_scroll_y,
                zoom_y: vp.zoom_y,
                viewport_y: vp.canvas_size.y.max(1.0),
            }),
            ruler: ArrangementRulerModel {
                scroll_x: vp.scroll_x,
                zoom_x: vp.zoom_x,
                ppq: self.editor.editor_state.view.ppq,
                left_inset,
                top_inset,
                time_signatures: self.editor.editor_state.data.time_signatures.clone(),
                total_ticks: vp.total_ticks,
            },
            lane: Some(ArrangementLaneModel {
                rect: lumino_message::types::geometry::Rect2::new(
                    vp.canvas_offset.x,
                    vp.canvas_offset.y,
                    vp.canvas_size.x,
                    vp.canvas_size.y,
                ),
                track_height: vp.track_height,
                tool: self.editor.current_tool(),
                selection: self
                    .editor
                    .editor_state
                    .data
                    .arrange_selection
                    .rects
                    .first()
                    .map(|&(ts, te, _kl, _kh, tl, th)| {
                        (ts as f64, te as f64, tl as usize, th as usize)
                    }),
                selected_notes: self
                    .editor
                    .arrangement_selected_notes()
                    .iter()
                    .copied()
                    .collect(),
                precision: self.toolbar.snap_spec,
            }),
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

impl Root {
    /// egui 轨道列动作 → 生产 `Message`。
    ///
    /// 与 旧版 `track_list/handlers.rs` 同语义：单击 = 选择 + 多选集合 + 注册拖拽候选；
    /// M/S 命中只切换开关；移动/释放交给生产侧的拖拽排序状态机。
    pub(crate) fn arrangement_action_message(
        &self,
        action: &ArrangementAction,
    ) -> Option<crate::Message> {
        use crate::sidebar::Event as SidebarEvent;
        Some(match action {
            ArrangementAction::Select { id, ids } => crate::Message::Batch(vec![
                SidebarEvent::track_selected(*id),
                SidebarEvent::tracks_selected(ids.clone()),
                SidebarEvent::track_reorder_started(*id),
            ]),
            ArrangementAction::ToggleMute(id) => SidebarEvent::track_mute_toggled(*id),
            ArrangementAction::ToggleSolo(id) => SidebarEvent::track_solo_toggled(*id),
            ArrangementAction::ReorderMoved { x, y } => SidebarEvent::track_reorder_moved(*x, *y),
            ArrangementAction::ReorderEnded => SidebarEvent::track_reorder_ended(None),
            ArrangementAction::CursorSet(tick) => {
                return Some(crate::Message::ArrangementCursorSet(*tick));
            }
            ArrangementAction::ZoomX { zoom, fixed_ratio } => {
                return Some(crate::Message::ArrangementZoomX {
                    zoom: *zoom,
                    fixed_ratio: *fixed_ratio,
                });
            }
            ArrangementAction::ScrollX(x) => {
                return Some(crate::Message::ArrangementScrollX(*x));
            }
            ArrangementAction::ScrollY(y) => {
                return Some(crate::Message::ArrangementScrollY(*y));
            }
            ArrangementAction::ZoomY { zoom, fixed_ratio } => {
                return Some(crate::Message::ArrangementZoomY {
                    zoom: *zoom,
                    fixed_ratio: *fixed_ratio,
                });
            }
            ArrangementAction::AddTrack => SidebarEvent::add_track(),
        })
    }
}

impl Root {
    /// 泳道动作（`lumino-arrangement-core` 的 `ArrangementAction`）→ 生产 `Message`。
    ///
    /// 与 旧版 `click_canvas` 的映射同一份语义：走带交互只有这一套实现，
    /// 两侧差异只在「事件怎么来」。
    pub(crate) fn lane_action_message(
        &self,
        action: &lumino_message::arrangement_action::ArrangementAction,
    ) -> crate::Message {
        use crate::sidebar::Event as SidebarEvent;
        action
            .clone()
            .into_message(|id| crate::Message::Sidebar(SidebarEvent::TrackSelected(id)))
    }
}
