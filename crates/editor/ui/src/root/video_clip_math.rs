//! 视频剪辑面板的几何与时间换算（**纯计算，无 旧版**）。
//!
//! 原 `view/video_clip/{layout,timeline}.rs` 的数学部分搬到这里：
//! - 渲染器入口面板（轨道占位 + 16:9 预览 + 时间轴 + 导出设置）的**精确矩形**；
//! - tick ↔ 秒换算（时间轴与导出锚点同源，避免两边漂移）。
//!
//! 这里取代了原先的 `rect_probe`：面板全部由 egui 绘制，几何必须自己算，
//! 不能再读 旧版 布局。布局常量与 旧版 原实现逐项一致（12 内边距、220 轨道列、
//! 12 列间距、40 标题 + 16:9 预览 + 200 时间轴 + 80 设置、4 行距、8 时间轴内边距）。

use egui::{Rect, Vec2, pos2};

/// 时间轴固定高度（与 旧版 原 `TIMELINE_HEIGHT` 一致）。
pub const TIMELINE_HEIGHT: f32 = 200.0;
/// 左侧轨道列表预留宽度。
pub const LEFT_RESERVED: f32 = 220.0;
/// 面板四周内边距。
pub const PANEL_PADDING: f32 = 12.0;
/// 左右两列间距（旧版 `row![left, right].spacing(12)`）。
pub const COLUMN_GAP: f32 = 12.0;
/// 标题行高度。
pub const HEADER_HEIGHT: f32 = 40.0;
/// 导出设置卡固定高度。
pub const SETTINGS_HEIGHT: f32 = 80.0;
/// 右侧列各行间距（header/preview/timeline/settings）。
pub const ROW_SPACING: f32 = 4.0;
/// 预览最小宽度。
pub const PREVIEW_MIN_W: f32 = 320.0;
/// 预览最小高度。
pub const PREVIEW_MIN_H: f32 = 180.0;
/// 播放头在时间轴内的固定 x 偏移。
pub const PLAYHEAD_X: f32 = 50.0;
/// 时间轴卡片内边距（`timeline_pane` 的 `.padding(8)`）。
pub const TIMELINE_PADDING: f32 = 8.0;
/// 时间轴标题行高度（容器 20 + 上下内边距 4×2）。
pub const TIMELINE_TITLE_H: f32 = 28.0;
/// 时间轴水平滚动条高度。
pub const TIMELINE_SCROLLBAR_H: f32 = 20.0;
/// 时间轴内部列间距（title / canvas / scrollbar）。
pub const TIMELINE_GAP: f32 = 4.0;

/// 渲染器入口面板的精确矩形（全部为窗口逻辑坐标）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RendererPanelLayout {
    /// 整块面板（右侧内容区）。
    pub area: Rect,
    /// 左侧轨道占位卡。
    pub left: Rect,
    /// 右侧列（header + preview + timeline + settings）。
    pub right: Rect,
    /// 标题行。
    pub header: Rect,
    /// 16:9 预览外层（铺 base 底色）。
    pub preview_wrapper: Rect,
    /// 16:9 预览黑盒（纹理铺放 / 占位文字）。
    pub preview: Rect,
    /// 时间轴卡片（含边框）。
    pub timeline: Rect,
    /// 时间轴标题行（传输按钮 / 信息）。
    pub timeline_title: Rect,
    /// 时间轴画布（素材带 / 播放头）。
    pub clip_canvas: Rect,
    /// 水平滚动条。
    pub timeline_scrollbar: Rect,
    /// 导出设置卡。
    pub settings: Rect,
}

/// 16:9 预览尺寸：在给定可用区域内取最大的 16:9 矩形（受最小尺寸兜底）。
pub fn preview_size(available_width: f32, available_height: f32) -> (f32, f32) {
    let aw = available_width.max(PREVIEW_MIN_W);
    let ah = available_height.max(PREVIEW_MIN_H);
    let mut w = aw;
    let mut h = w * 9.0 / 16.0;
    if h > ah {
        h = ah;
        w = h * 16.0 / 9.0;
    }
    if w < PREVIEW_MIN_W {
        w = PREVIEW_MIN_W;
        h = w * 9.0 / 16.0;
    }
    if h < PREVIEW_MIN_H {
        h = PREVIEW_MIN_H;
        w = h * 16.0 / 9.0;
    }
    (w, h)
}

/// 面板面积下的预览尺寸（扣除轨道列、行距与内边距后的 16:9）。
pub fn renderer_preview_size(area: Vec2) -> (f32, f32) {
    let available_w =
        (area.x - LEFT_RESERVED - COLUMN_GAP - PANEL_PADDING * 2.0).max(PREVIEW_MIN_W);
    let lower_reserved =
        HEADER_HEIGHT + TIMELINE_HEIGHT + SETTINGS_HEIGHT + ROW_SPACING * 3.0 + PANEL_PADDING * 2.0;
    let available_h = (area.y - lower_reserved).max(PREVIEW_MIN_H);
    preview_size(available_w, available_h)
}

/// 按真实面板矩形算出所有子块位置。
pub fn renderer_panel_layout(area: Rect) -> RendererPanelLayout {
    let inner = area.shrink(PANEL_PADDING);
    let left_w = LEFT_RESERVED.min(inner.width());
    let right_left = inner.left() + left_w + COLUMN_GAP;
    let right_width = (inner.right() - right_left).max(0.0);
    let right = Rect::from_min_size(
        pos2(right_left, inner.top()),
        Vec2::new(right_width, inner.height()),
    );

    let header = Rect::from_min_size(
        right.min,
        Vec2::new(right.width(), HEADER_HEIGHT.min(right.height())),
    );

    let (preview_w, preview_h) = renderer_preview_size(area.size());
    let preview_top = header.bottom() + ROW_SPACING;
    let preview_wrapper = Rect::from_min_size(
        pos2(right.left(), preview_top),
        Vec2::new(right.width(), preview_h),
    );
    let preview = Rect::from_min_size(
        pos2(
            preview_wrapper.left() + (preview_wrapper.width() - preview_w).max(0.0) * 0.5,
            preview_wrapper.top(),
        ),
        Vec2::new(preview_w, preview_h),
    );

    let timeline_top = preview_wrapper.bottom() + ROW_SPACING;
    let timeline = Rect::from_min_size(
        pos2(right.left(), timeline_top),
        Vec2::new(right.width(), TIMELINE_HEIGHT),
    );
    let timeline_inner = timeline.shrink(TIMELINE_PADDING);
    let timeline_title = Rect::from_min_size(
        timeline_inner.min,
        Vec2::new(timeline_inner.width(), TIMELINE_TITLE_H),
    );
    let timeline_scrollbar = Rect::from_min_size(
        pos2(
            timeline_inner.left(),
            timeline_inner.bottom() - TIMELINE_SCROLLBAR_H,
        ),
        Vec2::new(timeline_inner.width(), TIMELINE_SCROLLBAR_H),
    );
    let canvas_top = timeline_title.bottom() + TIMELINE_GAP;
    let clip_canvas = Rect::from_min_max(
        pos2(timeline_inner.left(), canvas_top),
        pos2(
            timeline_inner.right(),
            (timeline_scrollbar.top() - TIMELINE_GAP).max(canvas_top),
        ),
    );

    let settings = Rect::from_min_size(
        pos2(right.left(), timeline.bottom() + ROW_SPACING),
        Vec2::new(right.width(), SETTINGS_HEIGHT),
    );

    let left = Rect::from_min_size(inner.min, Vec2::new(left_w, inner.height()));

    RendererPanelLayout {
        area,
        left,
        right,
        header,
        preview_wrapper,
        preview,
        timeline,
        timeline_title,
        clip_canvas,
        timeline_scrollbar,
        settings,
    }
}

/// 预览显示状态（由「播放头是否在素材带内」×「纹理是否就绪」决定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewGate {
    /// 纹理就绪且播放头在带上：铺离屏纹理。
    Waterfall,
    /// 播放头不在素材带上。
    NoMaterial,
    /// 在带上但纹理尚未就绪。
    WaitingRender,
}

/// 预览状态判定（与 旧版 原 `preview_gate` 逐条一致）。
pub fn preview_gate(playhead_in_band: bool, texture_ready: bool) -> PreviewGate {
    match (playhead_in_band, texture_ready) {
        (false, _) => PreviewGate::NoMaterial,
        (true, true) => PreviewGate::Waterfall,
        (true, false) => PreviewGate::WaitingRender,
    }
}

/// 把 tick 按 tempo 变化表换算为秒。
pub fn ticks_to_seconds(tick: u64, ppq: u32, tempos: &[(u32, f32)]) -> f64 {
    if ppq == 0 {
        return tick as f64;
    }
    let mut total_secs = 0.0;
    let mut prev_tick: u32 = 0;
    let mut prev_bpm: f32 = 120.0;
    for &(t, bpm) in tempos {
        let segment_ticks = t.saturating_sub(prev_tick) as u64;
        let segment_secs = segment_ticks as f64 * 60.0 / (prev_bpm as f64 * ppq as f64);
        total_secs += segment_secs;
        if tick <= t as u64 {
            let within_ticks = tick.saturating_sub(prev_tick as u64);
            let within_secs = within_ticks as f64 * 60.0 / (prev_bpm as f64 * ppq as f64);
            return total_secs - segment_secs + within_secs;
        }
        prev_tick = t;
        prev_bpm = bpm;
    }
    let remaining = tick.saturating_sub(prev_tick as u64);
    total_secs + remaining as f64 * 60.0 / (prev_bpm as f64 * ppq as f64)
}

/// 工程总时长（空工程回退默认时长，至少 2s）。
pub fn duration_seconds(total_ticks: u32, ppq: u16, tempos: &[(u32, f32)]) -> f64 {
    if total_ticks == 0 {
        ticks_to_seconds(
            lumino_core::view_state::DEFAULT_TOTAL_TICKS as u64,
            ppq as u32,
            tempos,
        )
        .max(2.0)
    } else {
        ticks_to_seconds(total_ticks as u64, ppq as u32, tempos).max(1.0)
    }
}

/// 秒 → tick（与 [`ticks_to_seconds`] 互逆，误差 ≤1 tick）。
pub fn seconds_to_ticks(secs: f64, ppq: u32, tempos: &[(u32, f32)]) -> u64 {
    let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
    if ppq == 0 {
        return secs as u64;
    }
    let mut prev_tick: u32 = 0;
    let mut prev_bpm: f32 = 120.0;
    let mut acc_secs: f64 = 0.0;
    for &(t, bpm) in tempos {
        let seg_ticks = t.saturating_sub(prev_tick) as f64;
        let seg_secs = seg_ticks * 60.0 / (prev_bpm as f64 * ppq as f64);
        if secs <= acc_secs + seg_secs {
            let within = secs - acc_secs;
            let ticks = prev_tick as f64 + within * prev_bpm as f64 * ppq as f64 / 60.0;
            return ticks.max(0.0) as u64;
        }
        acc_secs += seg_secs;
        prev_tick = t;
        prev_bpm = bpm;
    }
    let ticks = prev_tick as f64 + (secs - acc_secs) * prev_bpm as f64 * ppq as f64 / 60.0;
    ticks.max(0.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn preview_is_strict_16_9() {
        for (w, h) in [
            (640.0, 360.0),
            (1280.0, 800.0),
            (1920.0, 1080.0),
            (1024.0, 768.0),
            (800.0, 600.0),
        ] {
            let (pw, ph) = preview_size(w, h);
            assert!(approx(pw / ph, 16.0 / 9.0), "{w}x{h}: {pw}x{ph}");
            assert!(pw <= w.max(PREVIEW_MIN_W) + 0.01);
            assert!(ph <= h.max(PREVIEW_MIN_H) + 0.01);
        }
    }

    #[test]
    fn preview_compresses_height_then_width() {
        let (w, h) = preview_size(1000.0, 180.0);
        assert!(approx(h, 180.0));
        assert!(approx(w, 320.0));

        let (w, h) = preview_size(320.0, 1000.0);
        assert!(approx(w, 320.0));
        assert!(approx(h, 180.0));
    }

    /// 布局：16:9 预览严格居中，时间轴子块按 旧版 原常量落位（无探针）。
    #[test]
    fn layout_matches_legacy_geometry() {
        let area = Rect::from_min_size(pos2(248.0, 30.0), Vec2::new(1800.0, 1182.0));
        let l = renderer_panel_layout(area);
        assert!(approx(l.left.left(), 248.0 + 12.0));
        assert!(approx(l.left.width(), 220.0));
        assert!(approx(l.right.left(), 248.0 + 12.0 + 220.0 + 12.0));
        assert!(approx(l.right.right(), area.right() - 12.0));

        // header 40，预览紧跟 4
        assert!(approx(l.header.height(), 40.0));
        assert!(approx(l.preview_wrapper.top(), l.header.bottom() + 4.0));
        assert!(approx(l.preview.center().x, l.preview_wrapper.center().x));
        assert!(approx(l.preview.width() / l.preview.height(), 16.0 / 9.0));

        // 时间轴 200，内边距 8，标题 28，滚动条 20，间距 4
        assert!(approx(l.timeline.top(), l.preview_wrapper.bottom() + 4.0));
        assert!(approx(l.timeline.height(), TIMELINE_HEIGHT));
        assert!(approx(l.timeline_title.height(), TIMELINE_TITLE_H));
        assert!(approx(
            l.timeline_scrollbar.bottom(),
            l.timeline.bottom() - TIMELINE_PADDING
        ));
        assert!(approx(
            l.clip_canvas.height(),
            TIMELINE_HEIGHT
                - TIMELINE_PADDING * 2.0
                - TIMELINE_TITLE_H
                - TIMELINE_SCROLLBAR_H
                - TIMELINE_GAP * 2.0
        ));

        // 设置卡 80，紧跟时间轴 4
        assert!(approx(l.settings.top(), l.timeline.bottom() + 4.0));
        assert!(approx(l.settings.height(), SETTINGS_HEIGHT));
    }

    #[test]
    fn duration_and_tick_conversion() {
        assert!((duration_seconds(960, 480, &[]) - 1.0).abs() < 0.001);
        assert!((ticks_to_seconds(960, 480, &[(480, 240.0)]) - 0.75).abs() < 0.001);
        assert_eq!(seconds_to_ticks(1.0, 480, &[]), 960);
        assert_eq!(seconds_to_ticks(-3.0, 480, &[]), 0);
        assert_eq!(seconds_to_ticks(f64::NAN, 480, &[]), 0);
        assert_eq!(seconds_to_ticks(100.0, 0, &[]), 100);
    }
}
