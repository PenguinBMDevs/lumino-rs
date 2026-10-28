//! 滚动增量帧一致性测试：逐像素比较"平移 + 窄条"合成与全量重画。

use std::sync::Arc;

use lumino_midi_loader::{MidiDocument, NoteEvent};

use super::canvas::shift_canvas_pixels;
use crate::{
    camera::Camera,
    model::TrackIndex,
    render::{Renderer, band::LodMode, color::NoteStyle, simd::SimdLevel},
};

#[test]
fn incremental_scroll_matches_full_render() {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    // 主轨：覆盖视口内外，含跨小节/长音符。
    for tick in (0..900).step_by(4) {
        for key in [60, 64, 67] {
            document.insert_note(0, NoteEvent::new(tick, tick + 2, key, 100, 0));
        }
    }
    document.insert_note(0, NoteEvent::new(0, 950, 62, 100, 0));
    // 洋葱皮轨。
    for tick in (0..900).step_by(6) {
        document.insert_note(1, NoteEvent::new(tick, tick + 4, 65, 100, 1));
    }

    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));
    let style = Arc::new(NoteStyle::new(2));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let mut renderer = Renderer::new(runtime, 2, style, SimdLevel::Scalar);

    let width = 600u32;
    let height = 240u32;
    renderer.resize(width, height);
    let mut camera = Camera::new(width, height, 1.0, 1200, 480, 1.0);
    camera.scroll_x = 0.0;
    camera.target_scroll_x = 0.0;
    camera.center_on_key(60);
    let keyboard = camera.keyboard_width.round() as usize;
    let ruler = camera.ruler_height.round() as usize;

    renderer.render(
        Arc::clone(&document),
        Arc::clone(&index),
        &camera,
        0,
        LodMode::Exact,
    );

    let step_px = 7i32;
    for step in 1..=8 {
        let frame = renderer.frame_mut();
        shift_canvas_pixels(frame, width as usize, height as usize, keyboard, step_px);
        camera.scroll_x = (step * step_px) as f32;
        camera.target_scroll_x = camera.scroll_x;
        renderer.render_strip_into_frame(
            Arc::clone(&document),
            Arc::clone(&index),
            &camera,
            0,
            LodMode::Exact,
            width - step_px as u32,
            width,
        );
    }
    let incremental = renderer.frame().to_vec();

    renderer.render(document, index, &camera, 0, LodMode::Exact);
    let full = renderer.frame();

    let mut diff = 0usize;
    let mut first = None;
    for y in ruler..height as usize {
        for x in keyboard..width as usize {
            let index = y * width as usize + x;
            if incremental[index] != full[index] {
                diff += 1;
                if first.is_none() {
                    first = Some((x, y, incremental[index], full[index]));
                }
            }
        }
    }
    assert_eq!(diff, 0, "incremental scroll mismatch: first={first:?}");
}

#[test]
fn fractional_scroll_uses_frame_scroll_not_camera_scroll() {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    for tick in (0..1200).step_by(3) {
        for key in [60, 62, 64, 65, 67] {
            document.insert_note(0, NoteEvent::new(tick, tick + 2, key, 100, 0));
        }
    }
    document.insert_note(0, NoteEvent::new(0, 1500, 60, 100, 0));
    for tick in (0..1200).step_by(5) {
        document.insert_note(1, NoteEvent::new(tick, tick + 3, 66, 100, 1));
    }

    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));
    let style = Arc::new(NoteStyle::new(2));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let mut renderer = Renderer::new(runtime, 2, style, SimdLevel::Scalar);

    let width = 600u32;
    let height = 240u32;
    renderer.resize(width, height);
    let mut camera = Camera::new(width, height, 1.0, 1600, 480, 1.0);
    camera.scroll_x = 0.0;
    camera.target_scroll_x = 0.0;
    camera.center_on_key(60);
    let keyboard = camera.keyboard_width.round() as usize;
    let ruler = camera.ruler_height.round() as usize;

    renderer.render(
        Arc::clone(&document),
        Arc::clone(&index),
        &camera,
        0,
        LodMode::Exact,
    );

    // 模拟真实动画：camera 每步走 7.4px，帧只能按整数像素平移。
    let step_px = 7.4_f32;
    let mut frame_scroll = 0.0_f32;
    for step in 1..=8 {
        let camera_scroll = step as f32 * step_px;
        let dx = camera_scroll - frame_scroll;
        let dx_int = dx.round() as i32;
        let frame = renderer.frame_mut();
        shift_canvas_pixels(frame, width as usize, height as usize, keyboard, dx_int);
        frame_scroll += dx_int as f32;
        camera.scroll_x = frame_scroll;
        camera.target_scroll_x = frame_scroll;
        renderer.render_strip_into_frame(
            Arc::clone(&document),
            Arc::clone(&index),
            &camera,
            0,
            LodMode::Exact,
            width - dx_int as u32,
            width,
        );
    }
    let incremental = renderer.frame().to_vec();

    renderer.render(document, index, &camera, 0, LodMode::Exact);
    let full = renderer.frame();

    let mut diff = 0usize;
    let mut first = None;
    for y in ruler..height as usize {
        for x in keyboard..width as usize {
            let index = y * width as usize + x;
            if incremental[index] != full[index] {
                diff += 1;
                if first.is_none() {
                    first = Some((x, y, incremental[index], full[index]));
                }
            }
        }
    }
    assert_eq!(diff, 0, "fractional scroll mismatch: first={first:?}");
}

#[test]
fn exact_render_keeps_fill_and_border() {
    let mut document = MidiDocument::empty_with_tracks(1, 480);
    document.insert_note(0, NoteEvent::new(0, 200, 60, 100, 0));
    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));
    let style = Arc::new(NoteStyle::new(1));
    let fill = style.fill(0, true);
    let border = style.border(0, true);
    assert_ne!(fill, border);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let mut renderer = Renderer::new(runtime, 1, style, SimdLevel::Scalar);
    let width = 400u32;
    let height = 200u32;
    renderer.resize(width, height);
    let mut camera = Camera::new(width, height, 1.0, 1000, 480, 1.0);
    camera.scroll_x = 0.0;
    camera.target_scroll_x = 0.0;
    camera.center_on_key(60);
    renderer.render(document, index, &camera, 0, LodMode::Exact);
    let frame = renderer.frame();

    let x_fill = camera.tick_to_x(100.0).round() as usize;
    let x_border = camera.tick_to_x(0.0).round() as usize + 1;
    let y = (camera.key_to_y(60) + camera.zoom_y * 0.5).round() as usize;
    assert_eq!(
        frame[y * width as usize + x_fill],
        fill,
        "中心像素必须是填充色"
    );
    assert_eq!(
        frame[y * width as usize + x_border],
        border,
        "边缘像素必须是描边色"
    );
}
