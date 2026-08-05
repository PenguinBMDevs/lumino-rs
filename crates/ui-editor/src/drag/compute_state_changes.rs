//! `compute_state_changes` 中各 match 分支的提取方法
//!
//! 将每个 `EditState` 变体的处理逻辑拆分为独立函数（≤30 行），
//! 使 `compute_state_changes()` 主方法 ≤40 行。

use lumino_editor_state::DragState;
use lumino_note_core::Note;
use std::cell::Cell;

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
    notes: &im::Vector<Note>,
    note_index: usize,
    snapped_tick: f32,
    snap_precision: f32,
) -> Option<f32> {
    notes
        .get(note_index)
        .map(|note| (snapped_tick - note.tick).max(snap_precision))
}

// ──────────────────────────────────────────────
//  DraggingSelection（批量拖动，ghost 方案）
// ──────────────────────────────────────────────

pub(super) fn handle_dragging_selection(
    drag_state: &mut DragState,
    key: u16,
    snapped_tick: f32,
    snap_precision: f32,
) {
    crate::puffin_profiler::dragging_selection();

    let raw_delta_tick = snapped_tick - drag_state.initial_tick as f32;
    let snapped_delta_tick = (raw_delta_tick / snap_precision).round() * snap_precision;
    let delta_tick_i = snapped_delta_tick as i64;
    let delta_key_i = (key as i32 - drag_state.initial_key as i32) as i16;

    if delta_tick_i != drag_state.delta_tick || delta_key_i != drag_state.delta_key {
        drag_state.set_delta(delta_tick_i, delta_key_i);
    }
}

/// 对选中的音符应用左边缘调整（tick 右移 + length 缩减）
fn apply_resize_start_to_selected(
    delta_tick: f32,
    snap_precision: f32,
    selected: &[usize],
    notes: &mut im::Vector<Note>,
) {
    for &i in selected {
        if let Some(note) = notes.get_mut(i) {
            let new_len = note.length - delta_tick;
            if new_len >= snap_precision {
                note.tick += delta_tick;
                note.length = new_len;
            }
        }
    }
}

/// 对选中的音符应用右边缘调整（length 增加）
fn apply_resize_end_to_selected(
    delta_tick: f32,
    snap_precision: f32,
    selected: &[usize],
    notes: &mut im::Vector<Note>,
) {
    for &i in selected {
        if let Some(note) = notes.get_mut(i) {
            let new_len = note.length + delta_tick;
            if new_len >= snap_precision {
                note.length = new_len;
            }
        }
    }
}

/// 批量调整左边缘：选中音符 tick 右移 + length 缩减
pub(super) fn handle_resizing_selection_start(
    last_tick: &mut f32,
    snapped_tick: f32,
    snap_precision: f32,
    selected: &[usize],
    notes: &mut im::Vector<Note>,
    selected_bounds: &Cell<Option<(f32, f32, u16, u16)>>,
) -> bool {
    let delta_tick = snapped_tick - *last_tick;
    if delta_tick == 0.0 {
        return false;
    }

    apply_resize_start_to_selected(delta_tick, snap_precision, selected, notes);
    *last_tick = snapped_tick;

    if let Some((min_t, max_te, max_k, min_k)) = selected_bounds.get() {
        selected_bounds.set(Some(((min_t + delta_tick).max(0.0), max_te, max_k, min_k)));
    }
    true
}

/// 批量调整右边缘：选中音符 length 增加
pub(super) fn handle_resizing_selection_end(
    last_tick: &mut f32,
    snapped_tick: f32,
    snap_precision: f32,
    selected: &[usize],
    notes: &mut im::Vector<Note>,
    selected_bounds: &Cell<Option<(f32, f32, u16, u16)>>,
) -> bool {
    let delta_tick = snapped_tick - *last_tick;
    if delta_tick == 0.0 {
        return false;
    }

    apply_resize_end_to_selected(delta_tick, snap_precision, selected, notes);
    *last_tick = snapped_tick;

    if let Some((min_t, max_te, max_k, min_k)) = selected_bounds.get() {
        selected_bounds.set(Some((min_t, max_te + delta_tick, max_k, min_k)));
    }
    true
}
