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
    /// 剪辑带时间轴未缩放基准宽度（像素）＝ 内容秒长 × 像素密度，最小兜底 400
    ///
    /// 内容秒长含素材带整体右移后的越界部分（见 [`Root::clip_timeline_content_secs`]），
    /// 与画布侧 `TimelineCanvas::content_width` 同源，滚动条不会比画布窄。
    pub(crate) fn clip_timeline_base_width(&self) -> f32 {
        (self.clip_timeline_content_secs()
            * crate::view::video_clip::timeline_canvas::PIXELS_PER_SEC)
            .max(400.0)
    }

    /// 剪辑带时间轴内容秒长（秒）：素材源长与两轨素材带右缘取大
    pub(crate) fn clip_timeline_content_secs(&self) -> f32 {
        let dur = self.clip_real_duration_secs() as f32;
        lumino_ui_core::state::video_clip_state::timeline_content_secs(
            dur,
            &[
                self.state.video_clip.video_edit,
                self.state.video_clip.audio_edit,
            ],
        )
    }

    /// 剪辑面板预览锚点 tick：`Some` = 播放头压在视频带上，画面取该时刻的帧。
    ///
    /// `None` = 播放头不在素材带上（带被裁短/右移后播放头落在带外），此时**必须
    /// 不显示瀑布流**——否则就是把素材带之外的内容当画面（越界显示）。
    ///
    /// 锚点用**素材源时间**（播放头 − 整体偏移）换算 tick：素材带整体右移后
    /// 时间轴时间与源时间不再相等，直接拿播放头当源时间会显示错帧。
    pub(crate) fn clip_preview_tick(&self) -> Option<u64> {
        let source_len = self.clip_real_duration_secs() as f32;
        let src_secs = self.state.video_clip.video_source_secs_at(source_len)?;
        Some(crate::view::video_clip::timeline::seconds_to_ticks(
            src_secs.max(0.0) as f64,
            self.editor.editor_state.view.ppq as u32,
            &self.tempo_pairs(),
        ))
    }

    /// 剪辑面板内容真实时长（秒）：文档轨尾标换算，空工程回退画布默认
    pub(crate) fn clip_real_duration_secs(&self) -> f64 {
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
    ///
    /// 播放指示线是**自由时间轴游标**：只有时间轴内容末尾才是它的边界，
    /// 素材带区间不参与截止（带外只是画面不显示，见 [`Root::clip_preview_tick`]）。
    pub(crate) fn tick_video_clip_transport(&mut self, dt_secs: f32) {
        if !self.is_renderer_entry_active() {
            return;
        }
        // 时间轴内容末尾（含素材带整体右移后的越界部分）——与标尺/滚动条同一权威值
        let content_secs = self.clip_timeline_content_secs();
        self.state
            .video_clip
            .advance_clip_transport(dt_secs, content_secs);
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
                // 标尺定位：写剪辑面板独立传输时钟（与卷帘完全无关）。
                // 播放头可落在时间轴任意 X 位置，不因素材带区间而受限；
                // 上限由画布侧的 ruler_click_secs 钳到时间轴内容末尾。
                self.state.video_clip.set_clip_position(secs);
                true
            }
            VideoClipAction::ClipPlayToggled => {
                let content_secs = self.clip_timeline_content_secs();
                let s = &mut self.state.video_clip;
                s.clip_toggle_play();
                // 已停在时间轴末尾时按播放 → 从 0 重新走，否则一按就停
                if s.clip_playing && s.clip_position_secs >= content_secs - f32::EPSILON {
                    s.clip_position_secs = 0.0;
                }
                true
            }
            VideoClipAction::ClipRewound => {
                // 回零 = 回到时间轴 0：播放头是自由游标，不随素材带头部移动
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
                // 播放头**不动**：素材带右移/裁短后它仍停在原时间轴位置。
                // 画面是否显示由 clip_preview_tick 按「播放头是否压在带上」决定——
                // 带不在播放头处就不显示瀑布流，无需搬动指示线。
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

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::storage::config::UiConfig;
    use lumino_message::video_clip::{ClipTrack, ClipTrimEdge};

    fn test_root() -> Root {
        Root::new(&UiConfig::default())
    }

    /// 期望的画面锚点 tick（与生产实现同一换算，用于对齐断言）
    fn expected_tick(root: &Root, source_secs: f32) -> u64 {
        crate::view::video_clip::timeline::seconds_to_ticks(
            source_secs as f64,
            root.editor.editor_state.view.ppq as u32,
            &root.tempo_pairs(),
        )
    }

    /// 需求 1：首端裁剪（素材带向右缩小）**不得**把播放指示线一起拖走。
    /// 需求 2：带不在播放头处 → 画面锚点为 None（上层据此不显示瀑布流）。
    #[test]
    fn test_trim_shrinks_band_without_dragging_playhead() {
        let mut root = test_root();
        let source = root.clip_real_duration_secs() as f32;
        assert!(source > 1.0, "工程兜底时长应 >1s，实际 {source}");

        // 播放头落在带上（默认带 = [0, source]）→ 有画面锚点
        root.handle_video_clip_action(VideoClipAction::TimelineSeek { secs: 1.0 });
        assert_eq!(root.clip_preview_tick(), Some(expected_tick(&root, 1.0)));

        // 首端裁掉一半 → 带起点右移，播放头必须原地不动
        let trim = (source * 0.5).min(5.0);
        assert!(trim > 1.0, "测试前提：裁剪量需大于播放头位置");
        root.handle_video_clip_action(VideoClipAction::ClipTrimChanged {
            track: ClipTrack::Video,
            edge: ClipTrimEdge::Start,
            trim_secs: trim,
        });
        assert!(
            (root.state.video_clip.clip_position_secs - 1.0).abs() < 1e-4,
            "首端裁剪不得移动播放头，实际 {}",
            root.state.video_clip.clip_position_secs
        );
        assert!(
            !root.state.video_clip.position_in_video_window(source),
            "播放头 1.0s 已落在带 [{trim}, {source}] 之外"
        );
        assert!(
            root.clip_preview_tick().is_none(),
            "带不在播放头处时不得给出画面锚点（否则会显示瀑布流）"
        );

        // 播放头移回带内 → 锚点恢复，且等于「播放头时刻」对应的帧
        root.handle_video_clip_action(VideoClipAction::TimelineSeek { secs: trim + 1.0 });
        assert_eq!(
            root.clip_preview_tick(),
            Some(expected_tick(&root, trim + 1.0)),
            "带内锚点应为播放头时刻对应的帧"
        );

        // 播放头可落在带之外的任意 X（自由游标，不受带区间约束）
        root.handle_video_clip_action(VideoClipAction::TimelineSeek { secs: 0.0 });
        assert_eq!(root.state.video_clip.clip_position_secs, 0.0);
        assert!(root.clip_preview_tick().is_none());

        // 回零 = 时间轴 0，不随带头移动
        root.handle_video_clip_action(VideoClipAction::TimelineSeek { secs: 2.0 });
        root.handle_video_clip_action(VideoClipAction::ClipRewound);
        assert!(root.state.video_clip.clip_position_secs.abs() < f32::EPSILON);
    }

    /// 需求 2 的另一半：素材带整体右移后，时间轴时间 ≠ 源时间，
    /// 画面锚点必须扣除偏移，否则显示的是错帧。
    #[test]
    fn test_offset_shifts_preview_frame_mapping() {
        let mut root = test_root();
        let source = root.clip_real_duration_secs() as f32;
        let offset = (source * 0.5).min(3.0);
        assert!(offset > 0.5, "测试前提：偏移需可观测");
        root.handle_video_clip_action(VideoClipAction::ClipTrackOffsetChanged {
            track: ClipTrack::Video,
            offset_secs: offset,
        });

        // 带左缘之前：无锚点
        root.handle_video_clip_action(VideoClipAction::TimelineSeek { secs: offset * 0.5 });
        assert!(
            root.clip_preview_tick().is_none(),
            "带左缘之前不得显示瀑布流"
        );

        // 带内：时间轴 offset+1s → 源 1s 的帧
        root.handle_video_clip_action(VideoClipAction::TimelineSeek { secs: offset + 1.0 });
        assert_eq!(
            root.clip_preview_tick(),
            Some(expected_tick(&root, 1.0)),
            "整体右移后锚点必须扣除偏移（否则错帧）"
        );
    }

    /// 回归：首端被向右缩短后，整体拖动必须能把素材带带回时间轴开头。
    ///
    /// 旧实现画布层与状态层各钳一次 `offset ≥ 0`，首裁 5s 后可视左缘永远
    /// ≥5s，用户只能拖到"开头缩短到的位置"，顶不到前面。现在边界只由
    /// `set_offset` 裁决：下限 = −首端裁剪（可视左缘顶到时间轴原点即止）。
    #[test]
    fn test_body_drag_can_return_band_to_timeline_origin() {
        let mut root = test_root();
        let source = root.clip_real_duration_secs() as f32;
        let trim = (source * 0.5).min(5.0);
        assert!(trim > 1.0, "测试前提：裁剪量需可观测");

        root.handle_video_clip_action(VideoClipAction::ClipTrimChanged {
            track: ClipTrack::Video,
            edge: ClipTrimEdge::Start,
            trim_secs: trim,
        });
        assert_eq!(
            root.state.video_clip.video_window(source),
            (trim, source),
            "首端裁短后带起点右移"
        );

        // 极端向前拖（画布原样发负值）：状态层应钳到 −trim，可视左缘顶到 0
        root.handle_video_clip_action(VideoClipAction::ClipTrackOffsetChanged {
            track: ClipTrack::Video,
            offset_secs: -999.0,
        });
        let edit = root.state.video_clip.video_edit;
        assert!(
            (edit.offset_secs + trim).abs() < 1e-4,
            "偏移下限应为 −首端裁剪（−{trim}），实际 {}",
            edit.offset_secs
        );
        assert!(
            edit.visible_start().abs() < 1e-4,
            "可视左缘必须能顶到时间轴原点，实际 {}",
            edit.visible_start()
        );
        assert_eq!(
            root.state.video_clip.video_window(source),
            (0.0, source - trim),
            "顶到原点后右缘随之左移（可视长度不变）"
        );

        // 时间轴 0 处显示的应是入点那一帧，而不是空占位
        root.handle_video_clip_action(VideoClipAction::TimelineSeek { secs: 0.0 });
        assert_eq!(
            root.clip_preview_tick(),
            Some(expected_tick(&root, trim)),
            "顶到原点后 0 处应显示入点帧"
        );
    }
}
