//! Step 5a 状态机验收：当前轨编辑跳过、洋葱轨单笔区域重绘、批量整帧、
//! 切轨补齐过期索引、版本/内容脏语义。
//!
//! 直接驱动 [`CpuOnionState`]（Host 只负责采集相机/静音/脏区并转发），
//! 这样每条语义都能在不构造窗口与 GPU 设备的前提下锁死。
//!
//! 本文件只放共享 fixture 与子模块声明；用例按主题拆分到 `tests_dirty/*`，
//! 保证每个测试文件 < 400 行（硬门槛，按真实行数计）。

use std::collections::HashSet;
use std::sync::Arc;

use lumino_editor_state::{CpuOnionChange, CpuOnionDirty};
use lumino_gfx::CpuOnionFrame;
use lumino_midi_model::{MidiDocument, NoteEvent};

use super::camera::{CpuOnionCameraInput, CpuOnionGeometry};
use super::state::{CpuOnionFrameKey, CpuOnionState};

const WIDTH: f32 = 640.0;
const HEIGHT: f32 = 240.0;

/// 三轨静音表：轨 0 当前、轨 1 洋葱皮、轨 2 静音。
const MUTED: [bool; 3] = [false, false, true];

/// 1:1 DPI、1px/tick、20px/key 的相机（可见键 116..127）。
fn geometry() -> CpuOnionGeometry {
    CpuOnionCameraInput {
        scale_factor: 1.0,
        canvas_offset: (0.0, 0.0),
        canvas_size: (WIDTH, HEIGHT),
        // 本文件的用例只关心内容区局部坐标，边栏取 0（帧矩形 = 整块画布）
        keyboard_width: 0.0,
        ruler_height: 0.0,
        scroll: (0.0, 0.0),
        zoom: (1.0, 20.0),
        key_count: 128,
    }
    .to_physical()
    .expect("100% 相机应可换算")
}

/// 帧冻结键（`gen_val` = `track_notes_gen`，`gen` 是保留字）。
fn key(gen_val: u64, current_track: usize) -> CpuOnionFrameKey {
    CpuOnionFrameKey {
        track_notes_gen: gen_val,
        current_track,
        mute_fingerprint: 7,
        palette_idx: 0,
        geometry: geometry(),
    }
}

/// 三轨文档：轨 0 = 当前轨、轨 1 = 洋葱皮轨、轨 2 = 静音轨。
fn document() -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(3, 480);
    document.insert_note(0, NoteEvent::new(10, 110, 120, 100, 0));
    document.insert_note(1, NoteEvent::new(10, 110, 121, 100, 0));
    document.insert_note(2, NoteEvent::new(10, 110, 122, 100, 0));
    document
}

/// 键位所在像素行的中心（避开 2px 描边）。
fn note_row(key: u16) -> u32 {
    (127u16.saturating_sub(key) as f32 * 20.0 + 10.0) as u32
}

fn alpha(frame: &CpuOnionFrame, x: u32, y: u32) -> u8 {
    let offset = ((y as usize) * frame.width as usize + x as usize) * 4;
    frame.pixels.get(offset + 3).copied().unwrap_or(0)
}

/// 直接 `track_notes_mut` 编辑（模拟拖动/批量编辑，不经插入删除 API）。
fn stretch_note(document: &mut MidiDocument, track: usize, index: usize, end_tick: u32) {
    if let Some(notes) = document.track_notes_mut(track)
        && let Some(note) = notes.get_mut(index)
    {
        note.end_tick = end_tick;
    }
}

fn change(track: usize, old: Option<NoteEvent>, new: Option<NoteEvent>) -> CpuOnionChange {
    CpuOnionChange { track, old, new }
}

fn dirty(changes: Vec<CpuOnionChange>) -> CpuOnionDirty {
    CpuOnionDirty {
        full: false,
        changes,
    }
}

fn tracks(ids: &[usize]) -> HashSet<usize> {
    ids.iter().copied().collect()
}

/// 用「冷启动一次整帧」得到参考像素。
fn reference_pixels(document: &MidiDocument, current_track: usize) -> Arc<Vec<u8>> {
    reference_pixels_with(document, current_track, &MUTED)
}

/// 指定静音表的参考像素（取消静音等布局变化用）。
fn reference_pixels_with(
    document: &MidiDocument,
    current_track: usize,
    muted: &[bool],
) -> Arc<Vec<u8>> {
    let mut state = CpuOnionState::default();
    state
        .update(key(1, current_track), document, muted, None, None)
        .frame
        .expect("冷启动必须生成帧")
        .pixels
}

/// 指定横向滚动的参考像素（冷启动整帧，验证增量滚动逐像素一致）。
fn reference_pixels_at(
    document: &MidiDocument,
    current_track: usize,
    scroll_x: f32,
) -> Arc<Vec<u8>> {
    let mut frame_key = key(1, current_track);
    frame_key.geometry.camera.scroll_x = scroll_x;
    let mut state = CpuOnionState::default();
    state
        .update(frame_key, document, &MUTED, None, None)
        .frame
        .expect("冷启动必须生成帧")
        .pixels
}

/// 当前轨直接编辑 / 切轨补齐（含同一帧内切轨）。
#[path = "tests_dirty/current.rs"]
mod current;

/// 洋葱轨脏区 / region / 整帧兜底。
#[path = "tests_dirty/onion.rs"]
mod onion;

/// 布局变化（滚动）与停用重启用。
#[path = "tests_dirty/frame.rs"]
mod frame;

/// 静音轨（直接编辑与几何补齐）。
#[path = "tests_dirty/mute.rs"]
mod mute;

/// Step 5b 脏区透传（`CpuOnionOutput::dirty` / `CpuOnionFrame::dirty_rect`）。
#[path = "tests_dirty/dirty_rect.rs"]
mod dirty_rect;
