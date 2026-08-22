//! 视频剪辑面板消息处理与数据源
//!
//! 剪辑带时间轴的时长权威值、**独立传输时钟**推进与交互动作处理。
//! 从 state_update 拆出（保持单文件 <400 行约束）。
//!
//! 2026-08 播放体系分离：剪辑面板持有秒域独立传输（clip_position_secs /
//! clip_playing），与卷帘的 tick 域 PlaybackManager 完全无关。

use crate::message::VideoClipAction;
use crate::root::Root;
use lumino_message::video_clip::ClipTrimEdge;

impl Root {
    /// 剪辑带时间轴未缩放基准宽度（像素）＝ MIDI 时长 × 像素密度，最小兜底 400
    pub(crate) fn clip_timeline_base_width(&self) -> f32 {
        (self.clip_real_duration_secs() as f32
            * crate::view::video_clip::timeline_canvas::PIXELS_PER_SEC)
            .max(400.0)
    }

    /// 剪辑面板内容真实时长（秒）：文档轨尾标换算，空工程回退画布默认
    fn clip_real_duration_secs(&self) -> f64 {
        let v = &self.editor.editor_state.view;
        let tempos = self.tempo_pairs();
        crate::view::video_clip::timeline::duration_seconds(
            self.clip_real_total_ticks(),
            v.ppq,
            &tempos,
        )
    }

    /// 剪辑面板是否处于首级入口（Renderer 分组且未进入子面板/瀑布流模式）
    pub(crate) fn is_renderer_entry_active(&self) -> bool {
        use crate::titlebar::mode_toggle::AppMode;
        use lumino_ui_core::sidebar_event::GroupId;
        self.sidebar.active_group == Some(GroupId::Renderer)
            && !self.sidebar.audio_export_visible
            && !self.sidebar.video_export_visible
            && self.state.current_mode != AppMode::Waterfall
    }

    /// 每帧推进剪辑面板**独立传输时钟**（秒域实时步进）。
    ///
    /// 与卷帘 PlaybackManager 完全无关——卷帘的播放/暂停/seek 不影响本时钟，
    /// 本时钟也不驱动卷帘走带。仅剪辑面板首级可见时推进；播放中滚动自动
    /// 跟随钉住走带线于区域前端 PLAYHEAD_X。
    pub(crate) fn tick_video_clip_transport(&mut self, dt_secs: f32) {
        if !self.is_renderer_entry_active() {
            return;
        }
        let duration = self.clip_real_duration_secs() as f32;
        self.state
            .video_clip
            .advance_clip_transport(dt_secs, duration);
        if self.state.video_clip.clip_playing {
            let pps_zoom = crate::view::video_clip::timeline_canvas::PIXELS_PER_SEC
                * self.state.video_clip.zoom;
            let pos = self.state.video_clip.clip_position_secs;
            self.state.video_clip.timeline_scroll_x =
                (pos * pps_zoom - crate::view::video_clip::layout::PLAYHEAD_X).max(0.0);
        }
    }

    /// 剪辑带真实内容总长（tick）：文档轨尾标优先，空工程回退画布默认值。
    ///
    /// 与播放引擎自动停止点（`tracks_max_end_tick`）同一权威值——视频带长度
    /// 恒等于播放实际走过的长度，加载 MIDI / 编辑音符后自然生效，无需额外同步。
    pub(crate) fn clip_real_total_ticks(&self) -> u32 {
        let doc_ticks = self
            .editor
            .editor_state
            .data
            .document
            .as_ref()
            .map(|doc| doc.tracks_max_end_tick())
            .unwrap_or(0);
        if doc_ticks > 0 {
            doc_ticks
        } else {
            self.editor.editor_state.view.total_ticks
        }
    }

    /// Tempo 映射 `(tick, bpm)` 列表（剪辑带时长/播放头换算共用）
    pub(crate) fn tempo_pairs(&self) -> Vec<(u32, f32)> {
        self.editor
            .editor_state
            .data
            .tempo_points
            .iter()
            .map(|tp| (tp.tick as u32, tp.bpm as f32))
            .collect()
    }

    /// 处理视频剪辑面板交互动作
    ///
    /// 全部动作只读写 [`VideoClipState`]（剪辑面板独立状态域），
    /// 不触碰卷帘的 `PlaybackManager` / `playback_position`。
    pub(crate) fn handle_video_clip_action(&mut self, action: VideoClipAction) -> bool {
        match action {
            VideoClipAction::ZoomChanged(factor) => {
                self.state.video_clip.apply_zoom(factor);
                true
            }
            VideoClipAction::ZoomSet(zoom) => {
                self.state.video_clip.set_zoom(zoom);
                true
            }
            VideoClipAction::PanChanged { dx, dy } => {
                self.state.video_clip.pan_by(dx, dy);
                true
            }
            VideoClipAction::ZoomAround {
                old_zoom,
                new_zoom,
                cursor_x,
                cursor_y,
                center_x,
                center_y,
            } => {
                self.state.video_clip.zoom_around(
                    old_zoom, new_zoom, 0.0, 0.0, center_x, center_y, cursor_x, cursor_y,
                );
                self.state.video_clip.set_zoom(new_zoom);
                true
            }
            VideoClipAction::ResetView => {
                self.state.video_clip.reset_view();
                true
            }
            VideoClipAction::TimelineSeek { secs } => {
                // 标尺定位：写剪辑面板独立传输时钟（与卷帘完全无关）
                self.state.video_clip.set_clip_position(secs);
                true
            }
            VideoClipAction::ClipPlayToggled => {
                self.state.video_clip.clip_toggle_play();
                true
            }
            VideoClipAction::ClipRewound => {
                self.state.video_clip.clip_rewind();
                true
            }
            VideoClipAction::ClipTrackOffsetChanged { track, offset_secs } => {
                self.state
                    .video_clip
                    .track_edit_mut(track)
                    .set_offset(offset_secs);
                true
            }
            VideoClipAction::ClipTrimChanged {
                track,
                edge,
                trim_secs,
            } => {
                let source_len = self.clip_real_duration_secs() as f32;
                let edit = self.state.video_clip.track_edit_mut(track);
                match edge {
                    ClipTrimEdge::Start => edit.set_trim_start(trim_secs, source_len),
                    ClipTrimEdge::End => edit.set_trim_end(trim_secs, source_len),
                }
                true
            }
            VideoClipAction::PreviewSizeChanged { width, height } => {
                self.state.video_clip.preview_width = width;
                self.state.video_clip.preview_height = height;
                true
            }
            VideoClipAction::TimelineScroll { x, viewport_w } => {
                let zoom = self.state.video_clip.zoom;
                let content_w = self.clip_timeline_base_width() * zoom;
                self.state
                    .video_clip
                    .set_timeline_scroll(x, content_w, viewport_w.max(1.0));
                true
            }
            VideoClipAction::TimelineZoom {
                zoom,
                fixed_ratio,
                viewport_w,
            } => {
                let old_zoom = self.state.video_clip.zoom;
                let base_w = self.clip_timeline_base_width();
                self.state.video_clip.timeline_zoom_around(
                    zoom,
                    fixed_ratio,
                    old_zoom,
                    base_w,
                    viewport_w.max(1.0),
                );
                true
            }
        }
    }
}
