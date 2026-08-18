//! DragState 单元测试

use super::DragState;
use bit_vec::BitVec;
use im::Vector;
use lumino_note_core::note::Note;

fn make_notes(count: usize) -> Vector<Note> {
    (0..count)
        .map(|idx| Note::new(idx as f32 * 10.0, 60 + idx as u16, 5.0))
        .collect()
}

#[test]
fn test_drag_state_new() {
    // from_elem 创建 3 个 bit 全 false
    let mut bv = BitVec::from_elem(3, false);
    bv.set(1, true);
    let drag_state = DragState::new(bv, 100, 60);
    assert_eq!(drag_state.delta_tick, 0);
    assert_eq!(drag_state.delta_key, 0);
    assert!(drag_state.has_selection());
    assert_eq!(drag_state.selected_count(), 1);
}

#[test]
fn test_drag_state_from_single() {
    let drag_state = DragState::from_single(2, 5, 50, 60);
    assert_eq!(drag_state.selected.len(), 5);
    assert!(!drag_state.selected[0]);
    assert!(!drag_state.selected[1]);
    assert!(drag_state.selected[2]);
    assert!(!drag_state.selected[3]);
    assert_eq!(drag_state.selected_count(), 1);
}

#[test]
fn test_drag_state_from_single_out_of_range() {
    let drag_state = DragState::from_single(10, 5, 50, 60);
    assert_eq!(drag_state.selected_count(), 0);
    assert!(!drag_state.has_selection());
}

#[test]
fn test_update_delta() {
    let mut drag_state = DragState::from_single(0, 1, 100, 60);
    drag_state.update_delta(150, 64);
    assert_eq!(drag_state.delta_tick, 50);
    assert_eq!(drag_state.delta_key, 4);
}

#[test]
fn test_set_delta() {
    let mut drag_state = DragState::default();
    drag_state.set_delta(-30, -5);
    assert_eq!(drag_state.delta_tick, -30);
    assert_eq!(drag_state.delta_key, -5);
    assert!(!drag_state.is_delta_zero());
}

#[test]
fn test_is_delta_zero() {
    let mut drag_state = DragState::default();
    assert!(drag_state.is_delta_zero());
    drag_state.set_delta(0, 1);
    assert!(!drag_state.is_delta_zero());
    drag_state.set_delta(1, 0);
    assert!(!drag_state.is_delta_zero());
    drag_state.set_delta(0, 0);
    assert!(drag_state.is_delta_zero());
}

#[test]
fn test_selected_indices() {
    // from_elem 创建 5 个 bit 全 false
    let mut bv = BitVec::from_elem(5, false);
    bv.set(1, true);
    bv.set(3, true);
    let drag_state = DragState::new(bv, 0, 0);
    assert_eq!(drag_state.selected_indices(), vec![1, 3]);
}

#[test]
fn test_ghost_position_basic() {
    let drag_state = DragState {
        selected: BitVec::new(),
        delta_tick: 50,
        delta_key: 5,
        initial_tick: 0,
        initial_key: 60,
    };
    let (ghost_tick, ghost_key) = drag_state.ghost_position(100.0, 60, 127);
    assert_eq!(ghost_tick, 150.0);
    assert_eq!(ghost_key, 65);
}

#[test]
fn test_ghost_position_clamps_negative_tick() {
    let drag_state = DragState {
        selected: BitVec::new(),
        delta_tick: -200,
        delta_key: 0,
        initial_tick: 0,
        initial_key: 60,
    };
    let (ghost_tick, _) = drag_state.ghost_position(100.0, 60, 127);
    assert_eq!(ghost_tick, 0.0, "tick 不应为负");
}

#[test]
fn test_ghost_position_clamps_key_range() {
    let drag_state = DragState {
        selected: BitVec::new(),
        delta_tick: 0,
        delta_key: -100,
        initial_tick: 0,
        initial_key: 60,
    };
    let (_, ghost_key) = drag_state.ghost_position(100.0, 60, 127);
    assert_eq!(ghost_key, 0, "key 不应小于 0");

    let other_drag_state = DragState {
        selected: BitVec::new(),
        delta_tick: 0,
        delta_key: 100,
        initial_tick: 0,
        initial_key: 60,
    };
    let (_, other_ghost_key) = other_drag_state.ghost_position(100.0, 100, 127);
    assert_eq!(other_ghost_key, 127, "key 不应超过 max_key");
}

#[test]
fn test_apply_to_notes_zero_delta_no_op() {
    let mut notes = make_notes(3);
    let original: Vec<_> = notes.iter().cloned().collect();
    let drag_state = DragState::from_single(0, 3, 0, 60);
    let modified = drag_state.apply_to_notes(&mut notes, 127);
    assert_eq!(modified, 0);
    for (idx, note) in notes.iter().enumerate() {
        assert_eq!(note.tick, original[idx].tick);
        assert_eq!(note.key, original[idx].key);
    }
}

#[test]
fn test_apply_to_notes_modifies_selected_only() {
    let mut notes = make_notes(3);
    // from_elem 创建 3 个 bit 全 false
    let mut bv = BitVec::from_elem(3, false);
    bv.set(0, true);
    bv.set(2, true);
    let ds = DragState {
        selected: bv,
        delta_tick: 100,
        delta_key: 7,
        initial_tick: 0,
        initial_key: 60,
    };
    let modified = ds.apply_to_notes(&mut notes, 127);
    assert_eq!(modified, 2);
    // note 0: tick=0, key=60 -> tick=100, key=67
    assert_eq!(notes[0].tick, 100.0);
    assert_eq!(notes[0].key, 67);
    // note 1: 未选中，不变
    assert_eq!(notes[1].tick, 10.0);
    assert_eq!(notes[1].key, 61);
    // note 2: tick=20, key=62 -> tick=120, key=69
    assert_eq!(notes[2].tick, 120.0);
    assert_eq!(notes[2].key, 69);
}

#[test]
fn test_apply_to_notes_clamps_negative_tick() {
    let mut notes = make_notes(1);
    let ds = DragState {
        selected: {
            let mut bv = BitVec::from_elem(1, false);
            bv.set(0, true);
            bv
        },
        delta_tick: -1000,
        delta_key: 0,
        initial_tick: 0,
        initial_key: 60,
    };
    ds.apply_to_notes(&mut notes, 127);
    assert_eq!(notes[0].tick, 0.0, "tick 应 clamp 到 0");
}

#[test]
fn test_resize_to_grow() {
    let mut drag_state = DragState::from_single(0, 2, 0, 60);
    drag_state.resize_to(5);
    assert_eq!(drag_state.selected.len(), 5);
    assert!(drag_state.selected[0]);
    assert!(!drag_state.selected[1]);
    assert!(!drag_state.selected[4]);
}

#[test]
fn test_resize_to_shrink() {
    // from_elem 创建 5 个 bit 全 true
    let bv = BitVec::from_elem(5, true);
    let mut drag_state = DragState::new(bv, 0, 60);
    drag_state.resize_to(3);
    assert_eq!(drag_state.selected.len(), 3);
    assert!(drag_state.selected[0]);
    assert!(drag_state.selected[2]);
}

#[test]
fn test_resize_to_same_size_noop() {
    let mut drag_state = DragState::from_single(1, 3, 0, 60);
    drag_state.resize_to(3);
    assert_eq!(drag_state.selected.len(), 3);
    assert!(drag_state.selected[1]);
}

#[test]
fn test_clear_resets_all() {
    let mut drag_state = DragState::from_single(0, 3, 100, 60);
    drag_state.set_delta(50, 5);
    drag_state.clear();
    assert!(!drag_state.has_selection());
    assert!(drag_state.is_delta_zero());
    assert_eq!(drag_state.selected.len(), 0);
}

#[test]
fn test_reset_delta_keeps_selection() {
    let mut drag_state = DragState::from_single(0, 3, 100, 60);
    drag_state.set_delta(50, 5);
    drag_state.reset_delta();
    assert!(drag_state.is_delta_zero());
    assert!(drag_state.has_selection(), "selected 应保留");
}
