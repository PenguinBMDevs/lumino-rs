//! CPU 洋葱皮帧生成验收：跳过当前轨/静音轨、帧尺寸 = 物理画布、帧冻结。
//!
//! 这里直接驱动 [`CpuOnionState`]（Host 侧只是收集相机/静音并转发），
//! 避免构造需要窗口与 GPU 设备的 `Host`——被测逻辑与被测输入完全一致。

use std::sync::Arc;

use lumino_gfx::CpuOnionFrame;
use lumino_midi_model::{MidiDocument, NoteEvent};

use super::camera::CpuOnionCameraInput;
use super::collect_muted_tracks;
use super::state::{CpuOnionFrameKey, CpuOnionState};
use crate::sidebar::Track;

/// 帧尺寸：640 × 240（zoom_y = 20 时可见 127..115 键，足够覆盖 120/121/122）。
const WIDTH: f32 = 640.0;
const HEIGHT: f32 = 240.0;

/// 1:1 DPI 的相机输入（画布铺满帧）。
fn camera_input(scale_factor: f32) -> CpuOnionCameraInput {
    CpuOnionCameraInput {
        scale_factor,
        canvas_offset: (0.0, 0.0),
        canvas_size: (WIDTH, HEIGHT),
        scroll: (0.0, 0.0),
        zoom: (1.0, 20.0),
        key_count: 128,
    }
}

/// 三轨文档：轨 0 = 当前轨，轨 1 = 洋葱皮轨，轨 2 = 静音轨，键位依次错开。
fn document() -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(3, 480);
    document.insert_note(0, NoteEvent::new(10, 110, 120, 100, 0));
    document.insert_note(1, NoteEvent::new(10, 110, 121, 100, 0));
    document.insert_note(2, NoteEvent::new(10, 110, 122, 100, 0));
    document
}

/// 键位所在的像素行（取行中心，避开 2px 描边）。
fn note_row(key: u16) -> u32 {
    (127u16.saturating_sub(key) as f32 * 20.0 + 10.0) as u32
}

/// 取像素 alpha（越界返回 0，测试里当透明处理）。
fn alpha(frame: &CpuOnionFrame, x: u32, y: u32) -> u8 {
    let offset = ((y as usize) * frame.width as usize + x as usize) * 4;
    frame.pixels.get(offset + 3).copied().unwrap_or(0)
}

fn frame_key(geometry: super::camera::CpuOnionGeometry) -> CpuOnionFrameKey {
    CpuOnionFrameKey {
        track_notes_gen: 1,
        current_track: 0,
        mute_fingerprint: 7,
        palette_idx: 0,
        geometry,
    }
}

/// 当前轨/静音轨必须透明，非当前非静音轨 alpha = 255，帧尺寸 = 物理画布。
#[test]
fn cpu_frame_skips_current_and_muted_tracks() {
    let document = document();
    let muted = vec![false, false, true];
    let geometry = camera_input(1.0).to_physical().expect("100% 相机应可换算");
    let mut state = CpuOnionState::default();

    let output = state.update(frame_key(geometry), &document, &muted);
    assert!(output.rebuilt, "首次调用必须重新光栅化");
    let frame = output.frame.expect("有文档且尺寸合法时必须生成帧");

    // 帧矩形/尺寸 = 物理 canvas（与 render_pass 的 canvas scissor 同口径）
    assert_eq!((frame.x, frame.y), (0, 0));
    assert_eq!(frame.size(), (WIDTH as u32, HEIGHT as u32));
    assert!(frame.validate(), "帧像素长度必须与尺寸自洽");

    assert_eq!(
        alpha(&frame, 60, note_row(120)),
        0,
        "当前轨由 GPU 负责，CPU 帧必须透明"
    );
    assert_eq!(
        alpha(&frame, 60, note_row(122)),
        0,
        "静音轨必须透明（muted[2] = true）"
    );
    assert_eq!(
        alpha(&frame, 60, note_row(121)),
        255,
        "非当前、非静音的洋葱皮轨必须绘制"
    );
    assert_eq!(alpha(&frame, 0, 0), 0, "背景必须全透明");
}

/// 帧矩形随 DPI 换算成物理像素（125%：全部 ×1.25 并向下取整）。
#[test]
fn cpu_frame_rect_is_physical_pixels() {
    let document = document();
    let muted = vec![false, false, true];
    let mut input = camera_input(1.25);
    input.canvas_offset = (10.0, 5.0);
    input.canvas_size = (200.0, 100.0);
    let geometry = input.to_physical().expect("125% 相机应可换算");
    // 10 × 1.25 = 12.5 → 12；5 × 1.25 = 6.25 → 6；200 × 1.25 = 250；100 × 1.25 = 125
    assert_eq!((geometry.x, geometry.y), (12, 6));
    assert_eq!((geometry.width, geometry.height), (250, 125));

    let mut state = CpuOnionState::default();
    let output = state.update(frame_key(geometry), &document, &muted);
    let frame = output.frame.expect("应生成帧");
    assert_eq!((frame.x, frame.y), (12, 6), "帧矩形必须是物理像素");
    assert_eq!(frame.size(), (250, 125), "帧尺寸必须是物理 canvas 尺寸");
    assert_eq!(
        state.frame().map(CpuOnionFrame::size),
        Some((250, 125)),
        "状态里冻结的帧必须与返回帧一致"
    );
}

/// 指纹不变 → 复用上一帧（不重渲染、不递增版本、像素 Arc 相同）。
#[test]
fn cpu_frame_is_frozen_until_inputs_change() {
    let document = document();
    let muted = vec![false, false, true];
    let geometry = camera_input(1.0).to_physical().expect("100% 相机应可换算");
    let mut state = CpuOnionState::default();
    let key = frame_key(geometry);

    let first = state.update(key, &document, &muted);
    assert!(first.rebuilt);
    let first = first.frame.expect("首帧应生成");

    // 同一指纹重复调用：不得重渲染、不得重传（版本相同 + 像素共享）
    let second = state.update(key, &document, &muted);
    assert!(!second.rebuilt, "指纹未变时不得重新光栅化");
    let second = second.frame.expect("应复用上一帧");
    assert_eq!(second.version, first.version, "复用帧的版本必须不变");
    assert!(
        Arc::ptr_eq(&first.pixels, &second.pixels),
        "复用帧必须共享同一份像素（不得整帧拷贝）"
    );

    // 文档代次变化 → 重建索引 + 重新光栅化 + 版本递增
    let mut edited_document = document.clone();
    edited_document.insert_note(1, NoteEvent::new(200, 210, 121, 100, 0));
    let mut next_key = key;
    next_key.track_notes_gen = 2;
    let third = state.update(next_key, &edited_document, &muted);
    assert!(third.rebuilt, "文档代次变化必须重新光栅化");
    let third = third.frame.expect("应生成新帧");
    assert_ne!(third.version, first.version, "新帧版本必须递增");
    assert!(
        !Arc::ptr_eq(&first.pixels, &third.pixels),
        "新帧必须是新的像素缓冲"
    );
}

/// 相机变化（仅滚动）同样要重渲染，且版本递增。
#[test]
fn cpu_frame_rebuilds_on_camera_change() {
    let document = document();
    let muted = vec![false, false, true];
    let base = camera_input(1.0).to_physical().expect("100% 相机应可换算");
    let mut state = CpuOnionState::default();

    let first = state.update(frame_key(base), &document, &muted);
    let first = first.frame.expect("首帧应生成");

    let mut scrolled_input = camera_input(1.0);
    scrolled_input.scroll = (100.0, 0.0);
    let scrolled = scrolled_input.to_physical().expect("相机应可换算");
    let second = state.update(frame_key(scrolled), &document, &muted);
    assert!(second.rebuilt, "滚动变化必须重新光栅化");
    assert_ne!(
        second.frame.expect("应生成新帧").version,
        first.version,
        "滚动后版本必须递增"
    );
}

/// 停用后释放缓存：帧消失，再次调用重新光栅化（版本从 1 重新开始）。
#[test]
fn cpu_frame_deactivate_releases_state() {
    let document = document();
    let muted = vec![false, false, true];
    let geometry = camera_input(1.0).to_physical().expect("100% 相机应可换算");
    let mut state = CpuOnionState::default();
    let key = frame_key(geometry);

    assert!(state.update(key, &document, &muted).frame.is_some());
    state.deactivate();
    assert!(state.frame().is_none(), "停用后不得保留帧");

    let rebuilt = state.update(key, &document, &muted);
    assert!(rebuilt.rebuilt);
    assert_eq!(
        rebuilt.frame.expect("停用后应可重建").version,
        1,
        "停用后版本重新从 1 开始"
    );
}

/// 静音映射：按文档轨数展开，侧栏越界 id 只进指纹集合、不进静音表。
#[test]
fn muted_mapping_is_bounds_safe() {
    let tracks = [track(0, true), track(1, false), track(5, true)];
    let (muted, ids) = collect_muted_tracks(&tracks, 3);
    assert_eq!(muted, vec![true, false, false], "越界 id 5 不得写入静音表");
    assert_eq!(
        ids,
        vec![0, 5],
        "越界 id 仍要进指纹（切回 GPU 后需要正确静音）"
    );
}

/// 测试用音轨（只关心 id 与静音位）。
fn track(id: usize, is_muted: bool) -> Track {
    Track {
        id,
        name: format!("Track{id}"),
        port: 0,
        channel: 0,
        display_label: format!("A{id:02}"),
        is_conductor: false,
        can_delete: true,
        is_muted,
        is_soloed: false,
        color: None,
    }
}
