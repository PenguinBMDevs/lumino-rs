//! `compute_state_changes` 中各 match 分支的提取方法
//!
//! 将每个 `EditState` 变体的处理逻辑拆分为独立函数（≤30 行），
//! 使 `compute_state_changes()` 主方法 ≤40 行。
//!
//! 2026-08 单一权威源：resize 相关函数直接操作 document 当前轨的
//! `&mut [NoteEvent]`（`track_notes_mut` 借用），不再操作 `im::Vector<Note>`。

use lumino_core::view_state::DEFAULT_PPQ;
use lumino_editor_state::DEFAULT_BPM;
use lumino_editor_state::DragState;
use lumino_editor_state::EditorData;
use lumino_editor_state::PreviewSequenceNote;
use lumino_midi_loader::ChunkedList;
use lumino_midi_loader::NoteEvent;
use std::time::{Duration, Instant};

// ──────────────────────────────────────────────
//  Drawing（仅更新 current_tick）
// ──────────────────────────────────────────────

pub(super) fn handle_drawing(current_tick: &mut f32, snapped_tick: f32) {
    *current_tick = snapped_tick;
}

// ──────────────────────────────────────────────
//  Dragging（单音符拖动，ghost 方案）
// ──────────────────────────────────────────────

pub(super) fn handle_dragging(
    drag_state: &mut DragState,
    last_played_key: &mut u16,
    tick: f32,
    key: u16,
    snap_precision: f32,
    visible_key_count: u16,
    original_pos: &Option<(f32, u16)>,
) -> Option<u16> {
    let Some((_, original_key)) = original_pos else {
        return None;
    };

    let raw_delta_tick = tick - drag_state.initial_tick as f32;
    let snapped_delta_tick = (raw_delta_tick / snap_precision).round() * snap_precision;
    let calculated_key = (key as i32 - drag_state.initial_key as i32 + *original_key as i32)
        .clamp(0, visible_key_count.saturating_sub(1) as i32) as u16;

    let delta_key = (calculated_key as i16).saturating_sub(*original_key as i16);
    drag_state.set_delta(snapped_delta_tick as i64, delta_key);

    if calculated_key != *last_played_key {
        *last_played_key = calculated_key;
        return Some(calculated_key);
    }
    None
}

// ──────────────────────────────────────────────
//  ResizingStart（调整左边缘）
// ──────────────────────────────────────────────

pub(super) fn handle_resizing_start(
    original_tick: f32,
    original_length: f32,
    snapped_tick: f32,
    snap_precision: f32,
) -> (Option<f32>, Option<f32>) {
    let end_tick = original_tick + original_length;
    let calculated_tick = snapped_tick.min(end_tick - snap_precision).max(0.0);
    (Some(calculated_tick), Some(end_tick - calculated_tick))
}

// ──────────────────────────────────────────────
//  ResizingEnd（调整右边缘）
// ──────────────────────────────────────────────

pub(super) fn handle_resizing_end(
    notes: &ChunkedList<NoteEvent>,
    note_index: usize,
    snapped_tick: f32,
    snap_precision: f32,
) -> Option<f32> {
    notes
        .get(note_index)
        .map(|note| (snapped_tick - note.start_tick as f32).max(snap_precision))
}

// ──────────────────────────────────────────────
//  DraggingSelection（批量拖动，ghost 方案）
// ──────────────────────────────────────────────

/// 更新批量拖动的 delta 偏移。
///
/// 返回 `Some(new_delta_key)` 当 **key 偏移发生变化**（含从非零变回 0）：
/// 调用方据此触发/停止批量拖动预览序列的发声反馈。
/// 仅 tick 偏移变化时不返回（纯水平移动不发声）。
pub(super) fn handle_dragging_selection(
    drag_state: &mut DragState,
    key: u16,
    snapped_tick: f32,
    snap_precision: f32,
) -> Option<i16> {
    crate::puffin_profiler::dragging_selection();

    let raw_delta_tick = snapped_tick - drag_state.initial_tick as f32;
    let snapped_delta_tick = (raw_delta_tick / snap_precision).round() * snap_precision;
    let delta_tick_i = snapped_delta_tick as i64;
    let delta_key_i = (key as i32 - drag_state.initial_key as i32) as i16;

    let key_changed = delta_key_i != drag_state.delta_key;
    if delta_tick_i != drag_state.delta_tick || key_changed {
        drag_state.set_delta(delta_tick_i, delta_key_i);
    }

    if key_changed { Some(delta_key_i) } else { None }
}

/// 构建批量拖动预览序列（发声反馈）。
///
/// - **正确时间顺序**：选中音符按 tick 升序排列（同 tick 保持选中位图顺序，稳定排序）；
/// - **当前 key 位置**：每个音符取拖动后的 ghost key（`原始 key + delta_key`，
///   clamp 到可见 key 范围，且不超出 u8 表达范围）；
/// - **BPM 时序**：各音符的 `play_at` 按相对首个音符的 tick 间隔 × 工程 BPM/PPQ
///   换算为真实时间（首个音符立即播放），由 `drain_preview_sequence` 按时弹出。
///
/// 返回待播放的序列（空 = 无有效选中音符）。
pub(super) fn build_preview_sequence(
    data: &EditorData,
    drag_state: &DragState,
    delta_key: i16,
    max_key: u16,
    now: Instant,
    velocity: u8,
) -> Vec<PreviewSequenceNote> {
    let max_key = max_key.min(u16::from(u8::MAX));
    let mut notes: Vec<(f32, u16)> = Vec::with_capacity(drag_state.selected_count());
    for idx in drag_state.selected_indices_fast() {
        if let Some(view) = data.get_note_view(idx) {
            let ghost_key = (view.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u16;
            notes.push((view.tick, ghost_key));
        }
    }
    notes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some(&(first_tick, _)) = notes.first() else {
        return Vec::new();
    };

    // 工程 BPM 与 PPQ：取首个音符所在 tick 处生效的 tempo（tempo_changes 按 tick 升序）
    let (bpm, division) =
        data.document
            .as_ref()
            .map_or((DEFAULT_BPM, u32::from(DEFAULT_PPQ)), |doc| {
                let bpm = doc
                    .tempo_changes
                    .iter()
                    .rev()
                    .find(|(t, _)| *t as f32 <= first_tick)
                    .map(|(_, b)| f64::from(*b))
                    .unwrap_or(DEFAULT_BPM);
                (bpm, u32::from(doc.division))
            });

    notes
        .into_iter()
        .map(|(tick, key)| PreviewSequenceNote {
            play_at: now
                + Duration::from_millis(tick_delay_millis(tick - first_tick, division, bpm)),
            key: key as u8,
            velocity,
        })
        .collect()
}

/// 将 tick 差换算为播放延迟（毫秒）：`tick / division = 四分音符数`，
/// `四分音符数 / bpm * 60_000 = 毫秒`。
fn tick_delay_millis(delta_ticks: f32, division: u32, bpm: f64) -> u64 {
    (f64::from(delta_ticks) * 60_000.0 / (f64::from(division) * bpm))
        .round()
        .max(0.0) as u64
}

/// 对选中的音符应用左边缘调整（tick 右移 + length 缩减）
///
/// 直接修改 document 当前轨的 NoteEvent（u32 tick）。delta 源自 snapped tick
/// 差值（整数网格），`as u32` 转换无损。
pub(crate) fn apply_resize_start_to_selected(
    delta_tick: f32,
    snap_precision: f32,
    selected: &[usize],
    notes: &mut ChunkedList<NoteEvent>,
) {
    for &i in selected {
        if let Some(note) = notes.get_mut(i) {
            let length = (note.end_tick - note.start_tick) as f32;
            let new_len = length - delta_tick;
            if new_len >= snap_precision {
                let new_start = (note.start_tick as f32 + delta_tick).max(0.0);
                note.start_tick = new_start as u32;
                note.end_tick = note.start_tick + new_len as u32;
            }
        }
    }
}

/// 对选中的音符应用右边缘调整（length 增加）
pub(crate) fn apply_resize_end_to_selected(
    delta_tick: f32,
    snap_precision: f32,
    selected: &[usize],
    notes: &mut ChunkedList<NoteEvent>,
) {
    for &i in selected {
        if let Some(note) = notes.get_mut(i) {
            let length = (note.end_tick - note.start_tick) as f32;
            let new_len = length + delta_tick;
            if new_len >= snap_precision {
                note.end_tick = note.start_tick + new_len as u32;
            }
        }
    }
}
