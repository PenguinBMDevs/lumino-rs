//! 视频剪辑面板消息处理与数据源
//!
//! 剪辑带时间轴的时长权威值、播放头换算、滚动跟随与交互动作处理。
//! 从 state_update 拆出（保持单文件 <400 行约束）。

use crate::message::VideoClipAction;
use crate::root::Root;

impl Root {
    /// 剪辑带时间轴未缩放基准宽度（像素）＝ MIDI 时长 × 像素密度，最小兜底 400
    pub(crate) fn clip_timeline_base_width(&self) -> f32 {
        let v = &self.editor.editor_state.view;
        let tempos = self.tempo_pairs();
        let duration = crate::view::video_clip::timeline::duration_seconds(
            self.clip_real_total_ticks(),
            v.ppq,
            &tempos,
        );
        (duration as f32 * crate::view::video_clip::timeline_canvas::PIXELS_PER_SEC).max(400.0)
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

    /// 当前播放头位置（秒）：播放帧缓存非阻塞读取 + tick→秒换算
    pub(crate) fn clip_playhead_secs(&self) -> f32 {
        let ppq = self.editor.editor_state.view.ppq;
        let tempos = self.tempo_pairs();
        let tick = self
            .playback
            .manager
            .as_ref()
            .and_then(|m| m.last_frame())
            .map(|f| f.tick)
            .unwrap_or(0.0);
        crate::view::video_clip::timeline::ticks_to_seconds(tick as u64, ppq as u32, &tempos) as f32
    }

    /// 剪辑带播放跟随：滚动位置钉住走带线于区域前端 `PLAYHEAD_X`，
    /// 使视频带/音频带以正确速度（`PIXELS_PER_SEC × zoom` px/s）向左流动。
    ///
    /// 由每帧 `update_playback_state` 在播放中调用。播放头不越过轨尾标，
    /// 故 `desired ≤ 内容宽 − PLAYHEAD_X` 天然成立，仅钳制下界即可。
    pub(crate) fn follow_video_clip_playhead(&mut self, tick: f32) {
        let ppq = self.editor.editor_state.view.ppq;
        let tempos = self.tempo_pairs();
        let secs =
            crate::view::video_clip::timeline::ticks_to_seconds(tick as u64, ppq as u32, &tempos)
                as f32;
        let pps_zoom =
            crate::view::video_clip::timeline_canvas::PIXELS_PER_SEC * self.state.video_clip.zoom;
        self.state.video_clip.timeline_scroll_x =
            (secs * pps_zoom - crate::view::video_clip::layout::PLAYHEAD_X).max(0.0);
    }

    /// 处理视频剪辑面板交互动作
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
