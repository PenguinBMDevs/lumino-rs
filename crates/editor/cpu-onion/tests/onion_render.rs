//! 洋葱皮 CPU 渲染验收：onion-only / 透明帧 / 滚动增量 / 填充与边框。

use std::sync::Arc;

use lumino_cpu_onion::{
    CanvasCamera, CpuOnionRenderer, TrackIndex,
    color::{BORDER_DARKEN_FACTOR, srgb_decode, srgb_encode},
};
use lumino_midi_model::{MidiDocument, NoteEvent};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 240;

fn camera(scroll_x: f32) -> CanvasCamera {
    CanvasCamera {
        width: WIDTH,
        height: HEIGHT,
        scroll_x,
        scroll_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 20.0,
        key_count: 128,
    }
}

fn note_row(camera: &CanvasCamera, key: u16) -> u32 {
    (camera.key_to_y(key) + camera.zoom_y * 0.5) as u32
}

fn is_transparent(pixel: [u8; 4]) -> bool {
    pixel == [0, 0, 0, 0]
}

/// 边框 = 填充 × 0.4（线性空间）；允许 sRGB 量化带来的 1 字节误差。
fn darkened(pixel: [u8; 4]) -> [u8; 4] {
    [
        srgb_encode(srgb_decode(pixel[0]) * BORDER_DARKEN_FACTOR),
        srgb_encode(srgb_decode(pixel[1]) * BORDER_DARKEN_FACTOR),
        srgb_encode(srgb_decode(pixel[2]) * BORDER_DARKEN_FACTOR),
        255,
    ]
}

#[test]
fn onion_only_skips_current_and_muted() {
    let mut document = MidiDocument::empty_with_tracks(3, 480);
    document.insert_note(0, NoteEvent::new(10, 110, 120, 100, 0));
    document.insert_note(1, NoteEvent::new(10, 110, 121, 100, 0));
    document.insert_note(2, NoteEvent::new(10, 110, 122, 100, 0));

    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));
    let mut renderer = CpuOnionRenderer::new(2, 3);
    renderer.resize(WIDTH, HEIGHT);
    let camera = camera(0.0);
    let frame = renderer.render_full(
        Arc::clone(&document),
        Arc::clone(&index),
        &camera,
        0,
        &[false, true, false],
    );

    for key in [120u16, 121] {
        let y = note_row(&camera, key);
        for x in [20u32, 60, 100] {
            let pixel = frame.pixel(x, y).expect("帧内像素");
            assert!(
                is_transparent(pixel),
                "当前轨/静音轨不得绘制: key={key} x={x} pixel={pixel:?}"
            );
        }
    }
    let y = note_row(&camera, 122);
    let pixel = frame.pixel(60, y).expect("帧内像素");
    assert_eq!(pixel[3], 255, "非当前、非静音轨必须绘制");
}

#[test]
fn out_of_range_current_and_short_muted_are_safe() {
    let mut document = MidiDocument::empty_with_tracks(3, 480);
    document.insert_note(0, NoteEvent::new(10, 110, 120, 100, 0));
    document.insert_note(1, NoteEvent::new(10, 110, 121, 100, 0));
    document.insert_note(2, NoteEvent::new(10, 110, 122, 100, 0));

    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));
    let mut renderer = CpuOnionRenderer::new(2, 3);
    renderer.resize(WIDTH, HEIGHT);
    let camera = camera(0.0);
    // current_track 越界：没有任何轨是当前轨；muted 只有 1 项：缺省 false。
    let frame = renderer.render_full(
        Arc::clone(&document),
        Arc::clone(&index),
        &camera,
        99,
        &[true],
    );

    let muted_row = note_row(&camera, 120);
    assert!(
        is_transparent(frame.pixel(60, muted_row).expect("帧内像素")),
        "muted[0] = true，轨道 0 必须透明"
    );
    for key in [121u16, 122] {
        let y = note_row(&camera, key);
        assert_eq!(
            frame.pixel(60, y).expect("帧内像素")[3],
            255,
            "muted 长度不足时缺省 false，轨道 {key} 必须绘制"
        );
    }
}

#[test]
fn frame_background_transparent_notes_alpha_255() {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(1, NoteEvent::new(100, 300, 120, 100, 0));

    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));
    let mut renderer = CpuOnionRenderer::new(2, 2);
    renderer.resize(WIDTH, HEIGHT);
    let camera = camera(0.0);
    let frame = renderer.render_full(Arc::clone(&document), Arc::clone(&index), &camera, 0, &[]);

    assert_eq!(frame.pixel(0, 0), Some([0, 0, 0, 0]), "背景必须全透明黑");
    let note = frame.pixel(200, note_row(&camera, 120)).expect("帧内像素");
    assert_eq!(note[3], 255, "音符 alpha 必须是 255");

    let mut opaque = 0u64;
    for pixel in frame.pixels.as_chunks::<4>().0 {
        let alpha = pixel[3];
        assert!(
            alpha == 0 || alpha == 255,
            "alpha 只允许 0（背景）或 255（音符），实际 {alpha}"
        );
        if alpha == 255 {
            opaque = opaque.saturating_add(1);
        }
    }
    assert!(opaque > 0, "必须至少有一个不透明音符像素");
    assert_eq!(frame.pixels.len(), (WIDTH * HEIGHT * 4) as usize);
}

#[test]
fn exact_fill_and_border() {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    // 宽音符：屏幕 x = [100, 300)，每侧 2px 边框，中间填充。
    document.insert_note(1, NoteEvent::new(100, 300, 120, 100, 0));
    // 窄音符：屏幕 x = [400, 403)，宽度 < 4px → 纯边框。
    document.insert_note(1, NoteEvent::new(400, 403, 120, 100, 0));

    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));
    let mut renderer = CpuOnionRenderer::new(2, 2);
    renderer.resize(WIDTH, HEIGHT);
    let camera = camera(0.0);
    let (border, fill) = {
        let frame =
            renderer.render_full(Arc::clone(&document), Arc::clone(&index), &camera, 0, &[]);
        let y = note_row(&camera, 120);

        assert_eq!(frame.pixel(99, y), Some([0, 0, 0, 0]), "音符左侧外应透明");
        let border = frame.pixel(100, y).expect("帧内像素");
        let fill = frame.pixel(200, y).expect("帧内像素");
        assert_ne!(fill, border, "填充色与边框色必须不同");
        assert_eq!(border[3], 255);
        assert_eq!(fill[3], 255);

        for x in 100..102 {
            assert_eq!(
                frame.pixel(x, y).expect("帧内像素"),
                border,
                "左侧 2px 边框"
            );
        }
        for x in 102..298 {
            assert_eq!(frame.pixel(x, y).expect("帧内像素"), fill, "中心填充");
        }
        for x in 298..300 {
            assert_eq!(
                frame.pixel(x, y).expect("帧内像素"),
                border,
                "右侧 2px 边框"
            );
        }
        assert_eq!(frame.pixel(300, y), Some([0, 0, 0, 0]), "音符右侧外应透明");

        for row in 140..142 {
            assert_eq!(
                frame.pixel(200, row).expect("帧内像素"),
                border,
                "上侧 2px 边框"
            );
        }
        for row in 142..158 {
            assert_eq!(
                frame.pixel(200, row).expect("帧内像素"),
                fill,
                "垂直中心填充"
            );
        }
        for row in 158..160 {
            assert_eq!(
                frame.pixel(200, row).expect("帧内像素"),
                border,
                "下侧 2px 边框"
            );
        }

        let expected = darkened(fill);
        for channel in 0..3 {
            assert!(
                border[channel].abs_diff(expected[channel]) <= 1,
                "边框必须为填充 ×0.4（线性空间）: channel={channel} border={} expected={}",
                border[channel],
                expected[channel]
            );
        }

        for x in 400..403 {
            assert_eq!(frame.pixel(x, y).expect("帧内像素"), border, "窄音符纯边框");
        }
        for row in 140..160 {
            assert_eq!(
                frame.pixel(401, row).expect("帧内像素"),
                border,
                "窄音符无填充，整高都是边框"
            );
        }
        assert_eq!(
            frame.pixel(403, y),
            Some([0, 0, 0, 0]),
            "窄音符右侧外应透明"
        );
        (border, fill)
    };
    assert_ne!(border, fill);
    assert_eq!(renderer.visible_notes(), 2);
}

struct Harness {
    renderer: CpuOnionRenderer,
    document: Arc<MidiDocument>,
    index: Arc<TrackIndex>,
    base: CanvasCamera,
    current_track: usize,
    muted: Vec<bool>,
    frame_scroll: f32,
}

impl Harness {
    fn new() -> Self {
        let document = Arc::new(complex_document());
        let index = Arc::new(TrackIndex::build(&document));
        let base = camera(0.0);
        let current_track = 0;
        let muted = vec![false, false, true, false];
        let mut renderer = CpuOnionRenderer::new(3, 4);
        renderer.resize(WIDTH, HEIGHT);
        renderer.render_full(
            Arc::clone(&document),
            Arc::clone(&index),
            &base,
            current_track,
            &muted,
        );
        Self {
            renderer,
            document,
            index,
            base,
            current_track,
            muted,
            frame_scroll: 0.0,
        }
    }

    /// 把相机滚动推进到 `target`（帧只能按整数像素平移）。
    fn advance_to(&mut self, target: f32) {
        let dx = (target - self.frame_scroll).round() as i32;
        if dx == 0 {
            return;
        }
        self.renderer.shift_frame(dx);
        self.frame_scroll += dx as f32;
        let mut strip_camera = self.base;
        strip_camera.scroll_x = self.frame_scroll;
        if dx > 0 {
            let x0 = WIDTH.saturating_sub(dx as u32);
            self.renderer.render_strip(
                Arc::clone(&self.document),
                Arc::clone(&self.index),
                &strip_camera,
                self.current_track,
                &self.muted,
                x0,
                WIDTH,
            );
        } else {
            let x1 = dx.unsigned_abs();
            self.renderer.render_strip(
                Arc::clone(&self.document),
                Arc::clone(&self.index),
                &strip_camera,
                self.current_track,
                &self.muted,
                0,
                x1,
            );
        }
    }

    fn assert_matches_full(&mut self) {
        let incremental = self.renderer.frame().pixels.clone();
        let mut full_camera = self.base;
        full_camera.scroll_x = self.frame_scroll;
        self.renderer.render_full(
            Arc::clone(&self.document),
            Arc::clone(&self.index),
            &full_camera,
            self.current_track,
            &self.muted,
        );
        let full = &self.renderer.frame().pixels;
        assert_eq!(incremental.len(), full.len());
        let mut diffs = 0usize;
        let mut first = None;
        for (index, (a, b)) in incremental.iter().zip(full.iter()).enumerate() {
            if a != b {
                diffs += 1;
                if first.is_none() {
                    let pixel = index / 4;
                    first = Some((
                        pixel % WIDTH as usize,
                        pixel / WIDTH as usize,
                        index % 4,
                        *a,
                        *b,
                    ));
                }
            }
        }
        assert_eq!(
            diffs, 0,
            "平移+窄条必须等于同滚动位置的全量渲染: first={first:?}, scroll={}",
            self.frame_scroll
        );
    }
}

fn complex_document() -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(4, 480);
    // 当前轨：必须被跳过。
    for tick in (0..4000).step_by(7) {
        document.insert_note(0, NoteEvent::new(tick, tick + 3, 120, 100, 0));
    }
    // 洋葱皮轨 1：密集短音符 + 一条跨越全窗口的长音符。
    for tick in (0..4000).step_by(5) {
        document.insert_note(1, NoteEvent::new(tick, tick + 4, 118, 100, 1));
    }
    document.insert_note(1, NoteEvent::new(0, 4200, 126, 100, 1));
    // 静音轨 2：必须被跳过。
    for tick in (0..4000).step_by(11) {
        document.insert_note(2, NoteEvent::new(tick, tick + 6, 122, 100, 2));
    }
    // 洋葱皮轨 3：与轨 1 在垂直方向错开，形成多 band 内容。
    for tick in (0..4000).step_by(9) {
        document.insert_note(3, NoteEvent::new(tick, tick + 2, 124, 100, 3));
    }
    document
}

#[test]
fn incremental_scroll_matches_full_render() {
    let mut harness = Harness::new();

    // 整数滚动。
    for step in 1..=6 {
        harness.advance_to(step as f32 * 13.0);
    }
    harness.assert_matches_full();

    // 小数滚动：相机目标每步 +7.4px，帧按 round 后的整数平移。
    let start = harness.frame_scroll;
    for step in 1..=8 {
        harness.advance_to(start + step as f32 * 7.4);
    }
    harness.assert_matches_full();

    // 反向滚动：内容右移，新露出的窄条在左侧。
    let start = harness.frame_scroll;
    for step in 1..=5 {
        harness.advance_to(start - step as f32 * 9.0);
    }
    harness.assert_matches_full();
}
