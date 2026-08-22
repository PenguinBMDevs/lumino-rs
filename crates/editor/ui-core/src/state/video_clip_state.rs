//! 视频剪辑面板状态（瀑布流预览）
//!
//! 模仿 nezha `piano_view::show` 的 zoom / pan 交互模型，
//! 为 Lumino 视频剪辑窗口提供独立的预览视口状态。
//!
//! 2026-08 播放体系分离：本状态持有**秒域独立传输时钟**
//! （[`VideoClipState::clip_position_secs`] 等），与钢琴卷帘的
//! tick 域 [`PlaybackManager`] 完全无关——两面板互不驱动。

/// 最小缩放倍数（对标 nezha MIN_ZOOM）
pub const VIDEO_CLIP_MIN_ZOOM: f32 = 1.0;
/// 最大缩放倍数（对标 nezha MAX_ZOOM）
pub const VIDEO_CLIP_MAX_ZOOM: f32 = 10.0;
/// 滚轮缩放系数（对标 nezha ZOOM_SCROLL_FACTOR）
pub const VIDEO_CLIP_ZOOM_SCROLL_FACTOR: f32 = 1.1;

/// 剪辑轨道种类（视频/音频双轨）。
///
/// 权威定义在 `lumino_message::video_clip`（跨层消息契约），此处 re-export。
pub use lumino_message::video_clip::ClipTrack;

/// 剪辑轨道素材编辑状态（首尾裁剪 + 整体移动）
///
/// 素材源长即曲目时长；可视区间 `[offset+trim_in, offset+source_len-trim_out]`。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClipTrackEdit {
    /// 素材整体时间偏移（秒，≥0）：决定素材在时间轴上的摆放起点
    pub offset_secs: f32,
    /// 首端裁剪（秒，≥0）：素材开头被裁掉的部分
    pub trim_in_secs: f32,
    /// 尾端裁剪（秒，≥0）：素材结尾被裁掉的部分
    pub trim_out_secs: f32,
}

impl ClipTrackEdit {
    /// 素材可视区间起点（秒）＝ 偏移 ＋ 首端裁剪
    pub fn visible_start(&self) -> f32 {
        self.offset_secs + self.trim_in_secs
    }

    /// 素材可视时长（秒）＝ 源长 − 首尾裁剪（下限 0）
    pub fn visible_len(&self, source_len: f32) -> f32 {
        (source_len - self.trim_in_secs - self.trim_out_secs).max(0.0)
    }

    /// 设置整体偏移（下限 0）
    pub fn set_offset(&mut self, offset_secs: f32) {
        self.offset_secs = offset_secs.max(0.0);
    }

    /// 设置首端裁剪（绝对值），钳制 `[0, 源长−尾裁]`
    pub fn set_trim_start(&mut self, trim_secs: f32, source_len: f32) {
        let max = (source_len - self.trim_out_secs).max(0.0);
        self.trim_in_secs = trim_secs.clamp(0.0, max);
    }

    /// 设置尾端裁剪（绝对值），钳制 `[0, 源长−首裁]`
    pub fn set_trim_end(&mut self, trim_secs: f32, source_len: f32) {
        let max = (source_len - self.trim_in_secs).max(0.0);
        self.trim_out_secs = trim_secs.clamp(0.0, max);
    }
}

/// 视频剪辑预览状态
///
/// 仅保存视口交互状态（zoom / pan），不持有纹理。
/// 渲染所需离屏纹理通过 `Host.waterfall_player.view` 共享。
#[derive(Debug, Clone)]
pub struct VideoClipState {
    /// 缩放倍数（剪辑带时间轴与 header 显示共用，保证 UI 一致）
    pub zoom: f32,
    /// 平移偏移（像素）
    pub pan_x: f32,
    /// 平移偏移（像素）
    pub pan_y: f32,
    /// 时间轴水平滚动位置（像素，来自滚动条/滚轮）
    pub timeline_scroll_x: f32,
    /// 预览区内容宽度（由 responsive 回调写入）
    pub preview_width: f32,
    /// 预览区内容高度
    pub preview_height: f32,
    /// 剪辑面板独立播放头（秒域）——与卷帘 PlaybackManager 完全无关
    pub clip_position_secs: f32,
    /// 剪辑面板独立播放状态
    pub clip_playing: bool,
    /// 视频轨素材编辑（偏移/首尾裁剪）
    pub video_edit: ClipTrackEdit,
    /// 音频轨素材编辑（偏移/首尾裁剪）
    pub audio_edit: ClipTrackEdit,
}

impl Default for VideoClipState {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoClipState {
    /// 创建默认状态
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            timeline_scroll_x: 0.0,
            preview_width: 0.0,
            preview_height: 0.0,
            clip_position_secs: 0.0,
            clip_playing: false,
            video_edit: ClipTrackEdit::default(),
            audio_edit: ClipTrackEdit::default(),
        }
    }

    /// 切换剪辑面板独立播放/暂停
    pub fn clip_toggle_play(&mut self) {
        self.clip_playing = !self.clip_playing;
    }

    /// 回零并停止（剪辑面板独立传输）
    pub fn clip_rewind(&mut self) {
        self.clip_playing = false;
        self.clip_position_secs = 0.0;
    }

    /// 定位剪辑面板播放头（下限 0）
    pub fn set_clip_position(&mut self, secs: f32) {
        self.clip_position_secs = secs.max(0.0);
    }

    /// 推进剪辑面板独立传输时钟（每帧调用，秒域实时步进）
    ///
    /// 到达内容末尾自动停止并钉在末尾。
    pub fn advance_clip_transport(&mut self, dt_secs: f32, content_duration: f32) {
        if !self.clip_playing {
            return;
        }
        self.clip_position_secs += dt_secs.max(0.0);
        if self.clip_position_secs >= content_duration {
            self.clip_position_secs = content_duration.max(0.0);
            self.clip_playing = false;
        }
    }

    /// 取指定轨道的可变编辑状态
    pub fn track_edit_mut(&mut self, track: ClipTrack) -> &mut ClipTrackEdit {
        match track {
            ClipTrack::Video => &mut self.video_edit,
            ClipTrack::Audio => &mut self.audio_edit,
        }
    }

    /// 重置视口（双击恢复）
    pub fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.timeline_scroll_x = 0.0;
    }

    /// 设置时间轴滚动位置并钳制到有效范围
    ///
    /// `content_w` 为缩放后内容总宽，`viewport_w` 为时间轴可视宽度；
    /// 有效范围 `[0, (content_w - viewport_w).max(0)]`。
    pub fn set_timeline_scroll(&mut self, x: f32, content_w: f32, viewport_w: f32) {
        let max_scroll = (content_w - viewport_w).max(0.0);
        self.timeline_scroll_x = x.clamp(0.0, max_scroll);
    }

    /// 时间轴锚点缩放：以 `fixed_ratio`（视口内横向比例）为锚点调整缩放，
    /// 保证锚点处的内容点在缩放前后屏幕位置不动，随后钳制滚动。
    pub fn timeline_zoom_around(
        &mut self,
        new_zoom: f32,
        fixed_ratio: f32,
        old_zoom: f32,
        content_base_w: f32,
        viewport_w: f32,
    ) {
        let new_zoom = new_zoom.clamp(VIDEO_CLIP_MIN_ZOOM, VIDEO_CLIP_MAX_ZOOM);
        if old_zoom <= 0.0 || new_zoom <= 0.0 {
            return;
        }
        let anchor_px = fixed_ratio * viewport_w;
        let content_x = self.timeline_scroll_x + anchor_px;
        let scale = new_zoom / old_zoom;
        let new_scroll = content_x * scale - anchor_px;
        self.zoom = new_zoom;
        let content_w = content_base_w * new_zoom;
        self.set_timeline_scroll(new_scroll, content_w, viewport_w);
    }

    /// 应用缩放（限制在 [MIN, MAX]）
    pub fn apply_zoom(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(VIDEO_CLIP_MIN_ZOOM, VIDEO_CLIP_MAX_ZOOM);
    }

    /// 设置缩放（直接赋值并 clamp）
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(VIDEO_CLIP_MIN_ZOOM, VIDEO_CLIP_MAX_ZOOM);
    }

    /// 平移
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.pan_x += dx;
        self.pan_y += dy;
    }

    /// 以锚点为中心缩放（模拟 nezha 鼠标锚点逻辑）
    #[allow(clippy::too_many_arguments)]
    pub fn zoom_around(
        &mut self,
        old_zoom: f32,
        new_zoom: f32,
        anchor_x: f32,
        anchor_y: f32,
        viewport_center_x: f32,
        viewport_center_y: f32,
        cursor_x: f32,
        cursor_y: f32,
    ) {
        let center_x = viewport_center_x;
        let center_y = viewport_center_y;
        let anchor_view_x = (cursor_x - center_x - self.pan_x) / old_zoom;
        let anchor_view_y = (cursor_y - center_y - self.pan_y) / old_zoom;
        let _ = (anchor_x, anchor_y);
        self.pan_x = cursor_x - center_x - new_zoom * anchor_view_x;
        self.pan_y = cursor_y - center_y - new_zoom * anchor_view_y;
    }

    /// 限制 pan 在最大可平移范围内
    pub fn clamp_pan(&mut self, available_w: f32, available_h: f32, base_w: f32, base_h: f32) {
        let scaled_w = base_w * self.zoom;
        let scaled_h = base_h * self.zoom;
        let excess_x = (scaled_w - available_w).max(0.0) / 2.0;
        let excess_y = (scaled_h - available_h).max(0.0) / 2.0;
        self.pan_x = self.pan_x.clamp(-excess_x, excess_x);
        self.pan_y = self.pan_y.clamp(-excess_y, excess_y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let s = VideoClipState::new();
        assert!((s.zoom - 1.0).abs() < f32::EPSILON);
        assert!((s.pan_x).abs() < f32::EPSILON);
    }

    #[test]
    fn test_zoom_clamp() {
        let mut s = VideoClipState::new();
        s.apply_zoom(100.0);
        assert!(s.zoom <= VIDEO_CLIP_MAX_ZOOM);
        s.set_zoom(0.1);
        assert!(s.zoom >= VIDEO_CLIP_MIN_ZOOM);
    }

    #[test]
    fn test_reset() {
        let mut s = VideoClipState::new();
        s.zoom = 5.0;
        s.pan_x = 100.0;
        s.reset_view();
        assert!((s.zoom - 1.0).abs() < f32::EPSILON);
        assert!((s.pan_x).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pan_clamp() {
        let mut s = VideoClipState::new();
        s.zoom = 2.0;
        s.pan_x = 1000.0;
        s.clamp_pan(800.0, 600.0, 400.0, 300.0);
        // scaled 800x600 vs available 800x600 -> excess 0 -> pan 0
        assert!((s.pan_x).abs() < f32::EPSILON);
    }

    #[test]
    fn test_timeline_scroll_clamp() {
        let mut s = VideoClipState::new();
        // 内容 2000 宽、视口 800 → 有效范围 [0, 1200]
        s.set_timeline_scroll(5000.0, 2000.0, 800.0);
        assert!((s.timeline_scroll_x - 1200.0).abs() < f32::EPSILON);
        s.set_timeline_scroll(-100.0, 2000.0, 800.0);
        assert!(s.timeline_scroll_x.abs() < f32::EPSILON);
        // 内容不足视口 → 恒为 0
        s.set_timeline_scroll(50.0, 400.0, 800.0);
        assert!(s.timeline_scroll_x.abs() < f32::EPSILON);
    }

    #[test]
    fn test_timeline_zoom_around_anchor_stays_put() {
        let mut s = VideoClipState::new();
        let base_w = 1600.0; // 未缩放内容宽
        let viewport = 800.0;
        // 先滚到 200，锚点在视口右端（ratio=1.0）
        s.set_timeline_scroll(200.0, base_w, viewport);
        let old_zoom = s.zoom;
        s.timeline_zoom_around(2.0, 1.0, old_zoom, base_w, viewport);
        // 锚点内容坐标：200+800=1000；放大 2 倍后该点仍在视口右端：
        // new_scroll = 1000*2 - 800 = 1200
        assert!((s.timeline_scroll_x - 1200.0).abs() < 0.01);
        assert!((s.zoom - 2.0).abs() < f32::EPSILON);

        // 缩小回 1.0（锚点左端 ratio=0）：左缘内容点 1200 等比映射 → scroll = 1200*0.5 = 600，
        // 且该内容点仍位于视口左缘（锚点不动性的本质）
        s.timeline_zoom_around(1.0, 0.0, s.zoom, base_w, viewport);
        assert!((s.timeline_scroll_x - 600.0).abs() < 0.01);
        assert!((s.zoom - 1.0).abs() < f32::EPSILON);

        // 钳制：继续放大 10 倍时 scroll 不越界
        s.timeline_zoom_around(10.0, 1.0, s.zoom, base_w, viewport);
        let max_scroll = (base_w * 10.0 - viewport).max(0.0);
        assert!(s.timeline_scroll_x <= max_scroll + 0.01);
    }

    #[test]
    fn test_reset_clears_timeline_scroll() {
        let mut s = VideoClipState::new();
        s.timeline_scroll_x = 300.0;
        s.reset_view();
        assert!(s.timeline_scroll_x.abs() < f32::EPSILON);
    }

    #[test]
    fn test_clip_track_edit_visible_window() {
        let mut e = ClipTrackEdit::default();
        // 偏移 2s、首裁 1s、尾裁 0.5s、源长 10s → 可视起点 [3]，可视长 = 10−1−0.5 = 8.5
        e.set_offset(2.0);
        e.set_trim_start(1.0, 10.0);
        e.set_trim_end(0.5, 10.0);
        assert!((e.visible_start() - 3.0).abs() < f32::EPSILON);
        assert!((e.visible_len(10.0) - 8.5).abs() < f32::EPSILON);

        // 首裁钳制上界 = 源长 − 尾裁 = 9.5（绝对值 API）
        e.set_trim_start(100.0, 10.0);
        assert!((e.trim_in_secs - 9.5).abs() < f32::EPSILON);

        // 偏移下限 0
        e.set_offset(-5.0);
        assert!(e.offset_secs.abs() < f32::EPSILON);
    }

    #[test]
    fn test_clip_transport_advance_and_auto_stop() {
        let mut s = VideoClipState::new();
        // 非播放态推进无效
        s.advance_clip_transport(0.5, 10.0);
        assert!(s.clip_position_secs.abs() < f32::EPSILON);

        // 播放推进累加
        s.clip_playing = true;
        s.advance_clip_transport(0.25, 10.0);
        s.advance_clip_transport(0.25, 10.0);
        assert!((s.clip_position_secs - 0.5).abs() < f32::EPSILON);
        assert!(s.clip_playing);

        // 到末尾自动停止并钉在末尾
        s.advance_clip_transport(99.0, 10.0);
        assert!((s.clip_position_secs - 10.0).abs() < f32::EPSILON);
        assert!(!s.clip_playing);
    }

    #[test]
    fn test_clip_rewind_and_toggle() {
        let mut s = VideoClipState::new();
        s.clip_playing = true;
        s.set_clip_position(7.0);
        s.clip_rewind();
        assert!((s.clip_position_secs).abs() < f32::EPSILON);
        assert!(!s.clip_playing);
        s.clip_toggle_play();
        assert!(s.clip_playing);
    }
}
