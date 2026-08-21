//! 视频剪辑窗口布局辅助
//!
//! 对标 nezha `panels.rs` 的四区布局常量与容器样式。
//! 所有尺寸计算集中于此，作为 UI 布局与离屏纹理尺寸的单一事实源。

use iced_core::Size;

/// 时间轴固定高度（对标 nezha transport 200）
pub const TIMELINE_HEIGHT: f32 = 200.0;

/// 左侧轨道列表面板预留宽度
pub const LEFT_RESERVED: f32 = 220.0;

/// 行内边距预留（row padding 12*2）
pub const H_RESERVED: f32 = 36.0;

/// 头部条高度（标题 + 缩放显示）
pub const HEADER_HEIGHT: f32 = 40.0;

/// 导出设置区固定高度（与下部占位计算严格一致，禁止 Fill 抢占预览空间）
pub const SETTINGS_HEIGHT: f32 = 80.0;

/// 面板内边距（上下）
pub const PANEL_PADDING: f32 = 12.0;

/// 纵向子项间距（header/preview/timeline/settings 三处间隔，与 right_col.spacing 一致）
pub const ROW_SPACING: f32 = 4.0;

/// 预览最小宽度
pub const PREVIEW_MIN_W: f32 = 320.0;

/// 预览最小高度
pub const PREVIEW_MIN_H: f32 = 180.0;

/// 计算 16:9 预览尺寸，严格保持 16:9，不被 UI 拉伸
///
/// `available_width` 为预览可用宽，`available_height` 为预览可用高；
/// 返回 `(w,h)` 满足 `w/h == 16/9` 且 `w <= available_width`、`h <= available_height`，
/// 被压缩时动态重算。
pub fn calculate_preview_size(available_width: f32, available_height: f32) -> (f32, f32) {
    let aw = available_width.max(PREVIEW_MIN_W);
    let ah = available_height.max(PREVIEW_MIN_H);
    let mut w = aw;
    let mut h = w * 9.0 / 16.0;
    if h > ah {
        h = ah;
        w = h * 16.0 / 9.0;
    }
    // 再次钳制最小值
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

/// 计算渲染器入口面板（视频剪辑窗口）主区域内的 16:9 预览尺寸
///
/// 输入为 responsive 回调收到的主内容区 `Size`；内部扣除左侧轨道面板、
/// 头部、时间轴、设置区与边距后，按 16:9 取宽高约束的最小值。
/// UI 布局与离屏纹理尺寸共用本函数，保证存储与显示同比例。
pub fn renderer_panel_preview_size(area: Size) -> (f32, f32) {
    let available_w = (area.width - LEFT_RESERVED - H_RESERVED).max(PREVIEW_MIN_W);
    // 下部 UI 占位：header + timeline + settings + spacing*3 + padding*2
    let lower_reserved =
        HEADER_HEIGHT + TIMELINE_HEIGHT + SETTINGS_HEIGHT + ROW_SPACING * 3.0 + PANEL_PADDING * 2.0;
    let available_h = (area.height - lower_reserved).max(PREVIEW_MIN_H);
    calculate_preview_size(available_w, available_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn test_preview_aspect_16_9_basic() {
        let (w, h) = calculate_preview_size(640.0, 360.0);
        assert!(approx_eq(w / h, 16.0 / 9.0), "w/h={} expected 16/9", w / h);
        assert!(w <= 640.0 + 0.01);
        assert!(h <= 360.0 + 0.01);
    }

    #[test]
    fn test_preview_aspect_16_9_compressed_width() {
        // 可用宽很小，预览应按宽算高
        let (w, h) = calculate_preview_size(320.0, 1000.0);
        assert!(approx_eq(w / h, 16.0 / 9.0));
        assert!(approx_eq(w, 320.0));
        assert!(approx_eq(h, 180.0));
    }

    #[test]
    fn test_preview_aspect_16_9_compressed_height() {
        // 可用高很小，预览应按高重算宽，保持 16:9
        let (w, h) = calculate_preview_size(1000.0, 180.0);
        assert!(approx_eq(w / h, 16.0 / 9.0));
        assert!(approx_eq(h, 180.0));
        assert!(approx_eq(w, 320.0));
    }

    #[test]
    fn test_preview_storage_and_widget_same_ratio() {
        // 模拟 UI 中预览画面与 widget 组件的存储与显示尺寸应一致且均为 16:9
        let available_w = 800.0;
        let available_h = 600.0;
        let (storage_w, storage_h) = calculate_preview_size(available_w, available_h);
        let (widget_w, widget_h) = calculate_preview_size(available_w, available_h);
        assert!(approx_eq(storage_w / storage_h, 16.0 / 9.0));
        assert!(approx_eq(widget_w / widget_h, 16.0 / 9.0));
        assert!(approx_eq(storage_w, widget_w));
        assert!(approx_eq(storage_h, widget_h));
    }

    #[test]
    fn test_preview_dynamic_recalc_when_squeezed() {
        // 下部 UI 挤占导致可用高变小，预览应动态重算仍保持 16:9
        let available_w = 800.0;
        let available_h_tall = 600.0;
        let (w1, h1) = calculate_preview_size(available_w, available_h_tall);
        let available_h_squeezed = 200.0;
        let (w2, h2) = calculate_preview_size(available_w, available_h_squeezed);
        assert!(approx_eq(w1 / h1, 16.0 / 9.0));
        assert!(approx_eq(w2 / h2, 16.0 / 9.0));
        // 被压缩时宽高应变小但比例不变
        assert!(w2 < w1);
        assert!(h2 < h1);
    }

    #[test]
    fn test_renderer_panel_preview_size_is_16_9() {
        // 多组主区域尺寸下，面板级计算结果必须严格 16:9
        let cases = [
            (1280.0, 800.0),
            (1920.0, 1080.0),
            (1600.0, 900.0),
            (1024.0, 768.0),
            (800.0, 600.0),
        ];
        for (w, h) in cases {
            let (pw, ph) = renderer_panel_preview_size(Size::new(w, h));
            assert!(
                approx_eq(pw / ph, 16.0 / 9.0),
                "main {w}x{h}: preview {}x{} ratio {} != 16/9",
                pw,
                ph,
                pw / ph
            );
        }
    }

    #[test]
    fn test_renderer_panel_preview_fits_reserved_area() {
        // 预览必须放得进扣除预留后的可用区（不溢出、不被裁切）
        let cases = [(1280.0, 800.0), (1920.0, 1080.0), (1024.0, 768.0)];
        for (w, h) in cases {
            let (pw, ph) = renderer_panel_preview_size(Size::new(w, h));
            let lower_reserved = HEADER_HEIGHT
                + TIMELINE_HEIGHT
                + SETTINGS_HEIGHT
                + ROW_SPACING * 3.0
                + PANEL_PADDING * 2.0;
            let avail_w = w - LEFT_RESERVED - H_RESERVED;
            let avail_h = h - lower_reserved;
            assert!(
                pw <= avail_w + 0.01,
                "main {w}x{h}: {pw} > avail_w {avail_w}"
            );
            assert!(
                ph <= avail_h + 0.01,
                "main {w}x{h}: {ph} > avail_h {avail_h}"
            );
        }
    }
}
