//! 密度/覆盖层适配：数据与光栅化来自 `lumino_cpu_onion::density`。
//!
//! 本文件只做三件事，不复制 crate 的分层、增量重建与逐像素采样：
//! 1. `Camera` → `CanvasCamera`：crate 只描述 canvas 区域，示例相机还带
//!    `keyboard_width` / `ruler_height`；
//! 2. chrome 背景：crate 输出透明 `0xAARRGGBB`，示例先铺标尺 / 黑键行 /
//!    标尺底边，只把不透明像素叠上去，绝不拿透明像素覆盖背景；
//! 3. 总览档位：`≥30` 小节仍按示例相机的 `total_ticks` 裁剪口径判定
//!    （crate 的 `CanvasCamera` 不知道文档总长，不能代替这一步）。

use lumino_cpu_onion::{
    CanvasCamera,
    color::NoteStyle as CpuNoteStyle,
    density::{OverviewScratch, uses_overview_for_range},
};

use crate::{
    camera::Camera,
    model::TrackIndex,
    render::chrome::{
        COLOR_BG, COLOR_BLACK_ROW, COLOR_KEYBOARD_EDGE, COLOR_RULER_BG, is_black_key,
    },
};

pub use lumino_cpu_onion::density::DensityPyramid;

/// 覆盖/热力图共用的可复用缓冲（避免每帧分配整帧临时像素）。
#[derive(Debug, Default)]
pub struct Scratch {
    /// crate 的逐列 / 逐行缓存。
    cpu: OverviewScratch,
    /// canvas 尺寸的透明叠加帧：crate 只接受无 stride 的整帧。
    canvas: Vec<u32>,
}

/// 示例相机 → crate 画布相机：去掉键盘栏与标尺偏移，滚动/缩放保持同口径。
///
/// `left` / `top` 是调用方实际使用的整数像素偏移（覆盖路径四舍五入、总览
/// 路径截断）。偏移可能是分数（DPI 缩放后），所以要把整数偏移与相机浮点
/// 偏移的残差折进 `scroll_*`，否则每条采样列/行会平移 <1px：
/// - `x = tick * zoom_x - scroll_x`（canvas 局部），对应示例 `x + left`；
/// - `y = (key_count - 1 - key) * zoom_y - scroll_y`（canvas 局部），
///   对应示例 `y + top`。
#[must_use]
pub fn canvas_camera(
    camera: &Camera,
    canvas_width: u32,
    canvas_height: u32,
    left: usize,
    top: usize,
) -> CanvasCamera {
    CanvasCamera {
        width: canvas_width.max(1),
        height: canvas_height.max(1),
        scroll_x: camera.scroll_x + left as f32 - camera.keyboard_width,
        scroll_y: camera.scroll_y + top as f32 - camera.ruler_height,
        zoom_x: camera.zoom_x,
        zoom_y: camera.zoom_y,
        key_count: camera.key_count,
    }
}

/// 是否应切到总览密度 LOD：同屏可见小节数 ≥ 30。
#[must_use]
pub fn uses_overview(index: &TrackIndex, camera: &Camera) -> bool {
    uses_overview_for_range(
        index,
        camera.zoom_x,
        camera.zoom_y,
        camera.visible_tick_range(),
    )
}

/// 彩色覆盖光栅是否可用于当前缩放（只看横向缩放，尺寸仅用于计数恒非零）。
#[must_use]
pub fn can_render_coverage(density: &DensityPyramid, camera: &Camera) -> bool {
    let canvas = canvas_camera(camera, camera.width.max(1), camera.height.max(1), 0, 0);
    density.can_render_coverage(&canvas)
}

/// 彩色覆盖光栅：先铺示例 chrome 背景，再叠加 crate 的透明覆盖层。
///
/// crate 的覆盖层按生产语义排除 `current_track`（主轨由 GPU 单独绘制）。
/// 示例没有独立主轨 pass，这里传 `usize::MAX` 让所有轨道都进入覆盖层；
/// 代价是当前轨显示轨道色而不是主轨蓝（示例降级，T-32c）。
pub fn render_coverage(
    density: &DensityPyramid,
    frame: &mut [u32],
    width: usize,
    height: usize,
    camera: &Camera,
    style: &CpuNoteStyle,
    scratch: &mut Scratch,
) {
    if width == 0 || height == 0 || frame.len() < width.saturating_mul(height) {
        return;
    }
    let ruler = (camera.ruler_height.round().max(0.0) as usize).min(height);
    let keyboard = (camera.keyboard_width.round().max(0.0) as usize).min(width);
    fill_chrome(frame, width, height, camera, ruler);

    let canvas_width = width - keyboard;
    let canvas_height = height - ruler;
    if canvas_width == 0 || canvas_height == 0 {
        return;
    }
    resize_canvas(scratch, canvas_width, canvas_height);
    let canvas = canvas_camera(
        camera,
        canvas_width as u32,
        canvas_height as u32,
        keyboard,
        ruler,
    );
    density.render_coverage_with_scratch(
        &mut scratch.canvas,
        canvas_width as u32,
        canvas_height as u32,
        &canvas,
        style,
        usize::MAX,
        &[],
        &mut scratch.cpu,
    );
    overlay_canvas(
        frame,
        width,
        &scratch.canvas,
        canvas_width,
        keyboard,
        ruler,
        height,
    );
}

/// 总览热力图：先铺示例 chrome 背景，再叠加 crate 的透明热力层。
pub fn render_overview(
    density: &DensityPyramid,
    frame: &mut [u32],
    width: usize,
    height: usize,
    camera: &Camera,
    scratch: &mut Scratch,
) {
    if width == 0 || height == 0 || frame.len() < width.saturating_mul(height) {
        return;
    }
    // 与迁移前的总览路径一致：标尺/键盘按截断取整（覆盖路径用四舍五入）。
    let ruler = (camera.ruler_height as usize).min(height);
    let keyboard = (camera.keyboard_width as usize).min(width);
    fill_chrome(frame, width, height, camera, ruler);

    let canvas_width = width - keyboard;
    let canvas_height = height - ruler;
    if canvas_width == 0 || canvas_height == 0 {
        return;
    }
    resize_canvas(scratch, canvas_width, canvas_height);
    let canvas = canvas_camera(
        camera,
        canvas_width as u32,
        canvas_height as u32,
        keyboard,
        ruler,
    );
    density.render_overview_with_scratch(
        &mut scratch.canvas,
        canvas_width as u32,
        canvas_height as u32,
        &canvas,
        &mut scratch.cpu,
    );
    overlay_canvas(
        frame,
        width,
        &scratch.canvas,
        canvas_width,
        keyboard,
        ruler,
        height,
    );
}

/// 铺 chrome 背景：标尺行、黑键行、标尺底边。
fn fill_chrome(frame: &mut [u32], width: usize, height: usize, camera: &Camera, ruler: usize) {
    for y in 0..height {
        let color = if y < ruler {
            COLOR_RULER_BG
        } else {
            let key = camera.y_to_key(y as f32 + 0.5);
            if is_black_key(key) {
                COLOR_BLACK_ROW
            } else {
                COLOR_BG
            }
        };
        if let Some(row) = frame.get_mut(y * width..(y + 1) * width) {
            row.fill(color);
        }
    }
    if ruler > 0
        && let Some(row) = frame.get_mut((ruler - 1) * width..ruler * width)
    {
        row.fill(COLOR_KEYBOARD_EDGE);
    }
}

/// 把 canvas 尺寸的透明叠加层合成到窗口帧：透明像素保留 chrome 背景。
fn overlay_canvas(
    frame: &mut [u32],
    width: usize,
    canvas: &[u32],
    canvas_width: usize,
    left: usize,
    top: usize,
    height: usize,
) {
    let needed = canvas_width.saturating_mul(height.saturating_sub(top));
    if canvas.len() < needed {
        return;
    }
    for y in top..height {
        let source_row = y - top;
        let source = &canvas[source_row * canvas_width..(source_row + 1) * canvas_width];
        let Some(row) = frame.get_mut(y * width + left..(y + 1) * width) else {
            continue;
        };
        for (destination, &pixel) in row.iter_mut().zip(source) {
            if pixel != 0 {
                // crate 内部是 0xAARRGGBB；softbuffer 帧是 0x00RRGGBB。
                *destination = pixel & 0x00FF_FFFF;
            }
        }
    }
}

/// 把共享缓冲调整到 canvas 尺寸（尺寸不变时零开销；crate 会整帧重写）。
fn resize_canvas(scratch: &mut Scratch, canvas_width: usize, canvas_height: usize) {
    let len = canvas_width.saturating_mul(canvas_height);
    if scratch.canvas.len() != len {
        scratch.canvas.resize(len, 0);
    }
}

#[cfg(test)]
mod tests {
    use lumino_cpu_onion::density::visible_measure_count_for_range;
    use lumino_midi_loader::{MidiDocument, NoteEvent};

    use super::*;

    fn camera() -> Camera {
        let mut camera = Camera::new(800, 600, 1.0, 10_000, 480, 0.01);
        camera.center_on_key(125);
        camera
    }

    #[test]
    fn canvas_camera_matches_ui_offsets() {
        let camera = camera();
        let ruler = camera.ruler_height.round() as usize;
        let keyboard = camera.keyboard_width.round() as usize;
        let canvas = canvas_camera(
            &camera,
            camera.width - keyboard as u32,
            camera.height - ruler as u32,
            keyboard,
            ruler,
        );
        for (x, y) in [(0usize, 0usize), (100, 17), (37, 55)] {
            let expected_tick = camera.x_to_tick((x + keyboard) as f32 + 0.5);
            let tick = canvas.x_to_tick(x as f32 + 0.5);
            assert!(
                (tick - expected_tick).abs() < 1e-4,
                "{tick} vs {expected_tick}"
            );
            let expected_key = camera.y_to_key_f32((y + ruler) as f32 + 0.5);
            let key = canvas.y_to_key_f32(y as f32 + 0.5);
            assert!((key - expected_key).abs() < 1e-4, "{key} vs {expected_key}");
        }
    }

    #[test]
    fn coverage_overlay_keeps_chrome_background() {
        let mut document = MidiDocument::empty_with_tracks(1, 480);
        document.insert_note(0, NoteEvent::new(0, 2000, 125, 100, 0));
        let density = DensityPyramid::build(&document, 1, 128);
        let camera = camera();
        assert!(can_render_coverage(&density, &camera));
        let style = CpuNoteStyle::new(1);
        let mut scratch = Scratch::default();
        let (width, height) = (800usize, 600usize);
        let mut frame = vec![0u32; width * height];
        render_coverage(
            &density,
            &mut frame,
            width,
            height,
            &camera,
            &style,
            &mut scratch,
        );
        assert!(
            frame.iter().all(|&pixel| pixel != 0),
            "chrome 背景不得被透明像素覆盖"
        );
        let fill = style.fill(0) & 0x00FF_FFFF;
        assert!(frame.contains(&fill), "覆盖层必须写入真实轨道色");
    }

    #[test]
    fn overview_overlay_keeps_chrome_background() {
        let mut document = MidiDocument::empty_with_tracks(1, 480);
        document.insert_note(0, NoteEvent::new(0, 2000, 125, 100, 0));
        let density = DensityPyramid::build(&document, 1, 128);
        let mut camera = Camera::new(800, 600, 1.0, 10_000, 480, 0.01);
        camera.center_on_key(125);
        let mut scratch = Scratch::default();
        let mut frame = vec![0u32; 800 * 600];
        render_overview(&density, &mut frame, 800, 600, &camera, &mut scratch);
        assert!(
            frame.iter().all(|&pixel| pixel != 0),
            "chrome 背景不得被透明像素覆盖"
        );
        let background = [
            COLOR_RULER_BG,
            COLOR_BLACK_ROW,
            COLOR_BG,
            COLOR_KEYBOARD_EDGE,
        ];
        assert!(
            frame.iter().any(|pixel| !background.contains(pixel)),
            "热力层必须写入可见颜色"
        );
    }

    #[test]
    fn overview_count_is_clamped_at_document_end() {
        let document = MidiDocument::empty_with_tracks(1, 480);
        let mut index = TrackIndex::build(&document);
        index.total_notes = 1;
        index.measure_counts = vec![1; 64];
        index.measure_ticks = 1920;
        let mut camera = Camera::new(800, 600, 1.0, 2 * 1920, 480, 0.01);
        camera.keyboard_width = 0.0;
        camera.zoom_x = 800.0 / (30.0 * 1920.0);
        // 视口宽 30 小节但曲尾只有 2 小节：示例相机必须按 total_ticks 裁剪。
        assert_eq!(
            visible_measure_count_for_range(&index, camera.visible_tick_range()),
            2
        );
        assert!(!uses_overview(&index, &camera));
    }

    #[test]
    fn overview_threshold_uses_camera_measure_count() {
        let document = MidiDocument::empty_with_tracks(1, 480);
        let mut index = TrackIndex::build(&document);
        index.total_notes = 1;
        index.measure_counts = vec![1; 64];
        index.measure_ticks = 1920;
        let mut camera = Camera::new(800, 600, 1.0, 1_000_000, 480, 0.01);
        camera.keyboard_width = 0.0;
        camera.zoom_x = 800.0 / (29.0 * 1920.0);
        assert_eq!(
            visible_measure_count_for_range(&index, camera.visible_tick_range()),
            29
        );
        assert!(!uses_overview(&index, &camera));
        camera.zoom_x = 800.0 / (30.0 * 1920.0);
        assert_eq!(
            visible_measure_count_for_range(&index, camera.visible_tick_range()),
            30
        );
        assert!(uses_overview(&index, &camera));
    }

    /// 空文档与非有限/非正缩放都不得进入总览档：它把宽度除以 zoom，一旦为零
    /// 或 NaN，可见小节数的除法会溢出成巨大值并误判。
    #[test]
    fn overview_rejects_empty_document_and_invalid_zoom() {
        let document = MidiDocument::empty_with_tracks(1, 480);
        let mut index = TrackIndex::build(&document);
        let mut camera = Camera::new(800, 600, 1.0, 1_000_000, 480, 0.01);
        camera.keyboard_width = 0.0;
        camera.zoom_x = 800.0 / (30.0 * 1920.0);

        // 空文档：`total_notes == 0` 直接拒绝，不看缩放。
        assert!(!uses_overview(&index, &camera));

        index.total_notes = 1;
        index.measure_counts = vec![1; 64];
        index.measure_ticks = 1920;
        assert!(uses_overview(&index, &camera));

        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            camera.zoom_x = bad;
            assert!(
                !uses_overview(&index, &camera),
                "zoom_x = {bad} 必须拒绝总览档"
            );
        }
        camera.zoom_x = 800.0 / (30.0 * 1920.0);
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            camera.zoom_y = bad;
            assert!(
                !uses_overview(&index, &camera),
                "zoom_y = {bad} 必须拒绝总览档"
            );
        }
    }
}
