//! 视频剪辑面板状态（瀑布流预览）
//!
//! 模仿 nezha `piano_view::show` 的 zoom / pan 交互模型，
//! 为 Lumino 视频剪辑窗口提供独立的预览视口状态。
//!
//! 2026-08 播放体系分离：本状态持有**秒域独立传输时钟**
//! （[`VideoClipState::clip_position_secs`] 等），与钢琴卷帘的
//! tick 域 `PlaybackManager`（lumino-midi-io）完全无关——两面板互不驱动。

/// 最小缩放倍数（对标 nezha MIN_ZOOM）
pub const VIDEO_CLIP_MIN_ZOOM: f32 = 1.0;
/// 最大缩放倍数（对标 nezha MAX_ZOOM）
pub const VIDEO_CLIP_MAX_ZOOM: f32 = 10.0;
/// 滚轮缩放系数（对标 nezha ZOOM_SCROLL_FACTOR）
pub const VIDEO_CLIP_ZOOM_SCROLL_FACTOR: f32 = 1.1;

/// 素材带最小时长（秒）。
///
/// 长度调整的硬下限：任何一次首/尾裁剪都不得把可视长度压到该值以下。
/// 否则素材带会被拖成零长——条身消失、两端把手重合，既没有抓手也没有
/// 「拖回来」的余地（用户感知为「素材带被我拖没了」）。
pub const MIN_CLIP_DURATION_SECS: f32 = 0.1;

/// 剪辑轨道种类（视频/音频双轨）。
///
/// 权威定义在 `lumino_message::video_clip`（跨层消息契约），此处 re-export。
pub use lumino_message::video_clip::ClipTrack;

/// 剪辑轨道素材编辑状态（首尾裁剪 + 整体移动）
///
/// 素材源长即曲目时长；可视区间 `[offset+trim_in, offset+source_len-trim_out]`。
///
/// **唯一不变量**：可视左缘不得越过时间轴原点，即 `offset + trim_in ≥ 0`。
/// 偏移可以为负——首端被裁掉的隐藏开头允许落到时间轴之外，这样素材带整体
/// 向前拖时才能顶到时间轴开头（否则首裁多少就永远停在多少）。
/// 该不变量由 [`ClipTrackEdit::set_offset`] 与 [`ClipTrackEdit::set_trim_start`]
/// 共同维持，**不要在 UI 层重复钳制**（两侧规则不一致会互相打架）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClipTrackEdit {
    /// 素材整体时间偏移（秒，下限 `−trim_in_secs`）：决定素材在时间轴上的摆放起点
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

    /// 素材可视区间终点（秒）＝ 偏移 ＋ 源长 − 尾端裁剪
    ///
    /// 恒等于 [`Self::visible_start`] ＋ [`Self::visible_len`]，
    /// 是「画面出入点」中出点的时间位置。
    pub fn visible_end(&self, source_len: f32) -> f32 {
        self.offset_secs + (source_len - self.trim_out_secs).max(0.0)
    }

    /// 素材带在时间轴上的摆放区间（秒）：`[offset + trim_in, offset + 源长 − trim_out]`
    ///
    /// 供 [`VideoClipState::video_source_secs_at`] 判定播放头是否压在素材带上。
    pub fn timeline_span(&self, source_len: f32) -> (f32, f32) {
        (self.visible_start(), self.visible_end(source_len))
    }

    /// 设置整体偏移（下限由**可视左缘**决定）
    ///
    /// 约束的不变量是「素材带可视左缘不得越过时间轴原点」：
    /// `visible_start = offset + trim_in ≥ 0` ⟺ `offset ≥ −trim_in`。
    ///
    /// 曾经钳成 `offset ≥ 0`——那是把不变量错安在「素材自身原点」上：首端一旦
    /// 被裁掉 5s，`visible_start` 就永远 ≥5s，素材带再也拖不回时间轴开头
    /// （用户感知为「只能拖到开头缩短到的位置，顶不到前面」）。被裁掉的隐藏
    /// 开头允许落在时间轴之外，这与专业剪辑软件一致。
    ///
    /// 上界由时间轴内容长度兜住（见 [`timeline_content_secs`]）：素材带整体
    /// 右移后时间轴内容随之扩展，越界部分依然可滚动可达，不产生不可达状态。
    pub fn set_offset(&mut self, offset_secs: f32) {
        let v = if offset_secs.is_finite() {
            offset_secs
        } else {
            0.0
        };
        self.offset_secs = v.max(-self.trim_in_secs);
    }

    /// 设置首端裁剪（绝对值），钳制 `[max(0, −offset), 源长−尾裁−最小时长]`
    ///
    /// 下限同样来自「可视左缘不越过时间轴原点」：素材带已经顶在原点时
    /// （`offset = −trim_in`），首端把手只能向右，不能向左再"露出"更多开头
    /// ——时间轴原点之前没有位置可放。
    pub fn set_trim_start(&mut self, trim_secs: f32, source_len: f32) {
        let lo = (-self.offset_secs).max(0.0);
        let hi = (source_len - self.trim_out_secs - MIN_CLIP_DURATION_SECS).max(lo);
        let v = if trim_secs.is_finite() { trim_secs } else { lo };
        self.trim_in_secs = v.clamp(lo, hi);
    }

    /// 设置尾端裁剪（绝对值），钳制 `[0, 源长−首裁−最小时长]`
    pub fn set_trim_end(&mut self, trim_secs: f32, source_len: f32) {
        let max = (source_len - self.trim_in_secs - MIN_CLIP_DURATION_SECS).max(0.0);
        let v = if trim_secs.is_finite() {
            trim_secs
        } else {
            0.0
        };
        self.trim_out_secs = v.clamp(0.0, max);
    }
}

/// 剪辑带时间轴内容秒长（秒）：素材源长与各轨素材带右缘的较大值。
///
/// 时间轴的可滚动内容以素材源长为基准；素材带被整体右移后其右缘会越过源长，
/// 此时内容长度必须随之扩展，否则越界部分滚不到——素材带拖出去就再也拖不
/// 回来（状态越界）。UI 滚动条与状态层钳制共用本函数，保证两侧同源。
pub fn timeline_content_secs(source_len: f32, edits: &[ClipTrackEdit]) -> f32 {
    let source_len = if source_len.is_finite() {
        source_len.max(0.0)
    } else {
        0.0
    };
    edits
        .iter()
        .fold(source_len, |acc, e| acc.max(e.visible_end(source_len)))
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
    ///
    /// 回零 = 回到时间轴 0。播放指示线是**自由时间轴游标**：不跟随素材带头部，
    /// 素材带被裁掉开头也不改变回零位置。
    pub fn clip_rewind(&mut self) {
        self.clip_playing = false;
        self.clip_position_secs = 0.0;
    }

    /// 定位剪辑面板播放头（下限 0）
    ///
    /// 播放头可落在时间轴任意 X 位置（0 到内容末尾），不受素材带区间约束。
    pub fn set_clip_position(&mut self, secs: f32) {
        self.clip_position_secs = secs.max(0.0);
    }

    /// 视频轨素材带在时间轴上的摆放区间 `(起点, 终点)`（秒）。
    ///
    /// 音视频双轨共用同一秒域传输时钟；画面以视频轨区间为准。
    pub fn video_window(&self, source_len: f32) -> (f32, f32) {
        self.video_edit.timeline_span(source_len)
    }

    /// 播放头压在视频带上时返回其对应的**素材源时间**（秒），否则 `None`。
    ///
    /// 素材带整体摆放在时间轴 `[offset, offset + 源长]`，故源时间 = 播放头 − 偏移；
    /// 再按首尾裁剪判定该源时间是否落在可视区间 `[trim_in, 源长 − trim_out]` 内。
    /// 区间闭包：播放头正好压在入点/出点时仍在带内（那一帧可见）。
    ///
    /// 返回值是**画面该显示哪一帧**的唯一依据：`None` 就必须不显示瀑布流，
    /// 否则会把素材带之外的内容当成画面（越界显示）。
    pub fn video_source_secs_at(&self, source_len: f32) -> Option<f32> {
        let edit = self.video_edit;
        let src = self.clip_position_secs - edit.offset_secs;
        let hi = (source_len - edit.trim_out_secs).max(0.0);
        (src >= edit.trim_in_secs && src <= hi).then_some(src)
    }

    /// 播放头是否压在视频带上（决定预览是否显示瀑布流）
    pub fn position_in_video_window(&self, source_len: f32) -> bool {
        self.video_source_secs_at(source_len).is_some()
    }

    /// 推进剪辑面板独立传输时钟（每帧调用，秒域实时步进）
    ///
    /// `content_secs` 为时间轴内容末尾（秒）：走到末尾自动停止并钉在末尾。
    /// 播放头**不因素材带区间而截止**——素材带之外只是画面不显示，
    /// 时间轴本身照常走到底。
    pub fn advance_clip_transport(&mut self, dt_secs: f32, content_secs: f32) {
        if !self.clip_playing {
            return;
        }
        self.clip_position_secs += dt_secs.max(0.0);
        if self.clip_position_secs >= content_secs {
            self.clip_position_secs = content_secs.max(0.0);
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
mod tests;
