//! rendering 模块单元测试
//!
//! 从 rendering.rs 主文件拆出，因为主文件接近 400 行限制。

use super::ghost::ghost_delta_for_index;
use crate::EditState;
use lumino_editor_state::DragState;

fn drag_state_with_selected(indices: &[usize], total: usize, dt: i64, dk: i16) -> DragState {
    let mut ds = DragState::from_indices(indices.iter().copied(), total, 0, 0);
    ds.set_delta(dt, dk);
    ds
}

#[test]
fn ghost_delta_idle_with_pending_returns_pending_delta() {
    let pending = Some(drag_state_with_selected(&[1], 4, 120, 3));
    let edit_state = EditState::Idle;

    assert_eq!(
        ghost_delta_for_index(1, &pending, &edit_state),
        Some((120, 3))
    );
    assert_eq!(ghost_delta_for_index(0, &pending, &edit_state), None);
}

#[test]
fn ghost_delta_dragging_returns_drag_state_delta() {
    let drag_state = drag_state_with_selected(&[2], 4, 240, -5);
    let edit_state = EditState::Dragging {
        note_index: 2,
        drag_state,
        last_played_key: 0,
    };

    assert_eq!(
        ghost_delta_for_index(2, &None, &edit_state),
        Some((240, -5))
    );
    assert_eq!(ghost_delta_for_index(1, &None, &edit_state), None);
}

#[test]
fn ghost_delta_dragging_selection_merges_pending_and_drag() {
    let pending = Some(drag_state_with_selected(&[1, 2], 4, 100, 2));
    let drag_state = drag_state_with_selected(&[1, 2], 4, 50, 3);
    let edit_state = EditState::DraggingSelection { drag_state };

    // 选中音符合并两个 delta
    assert_eq!(
        ghost_delta_for_index(1, &pending, &edit_state),
        Some((150, 5))
    );
    // 未选中音符无 delta
    assert_eq!(ghost_delta_for_index(0, &pending, &edit_state), None);
}

#[test]
fn ghost_delta_selecting_or_resizing_applies_pending() {
    // pending 代表已启动异步提交但尚未完成的数据更新，在异步完成前应始终可见，
    // 不应因进入 Selecting / Resizing 等状态而回撤。
    let pending = Some(drag_state_with_selected(&[0], 2, 10, 1));
    let selecting = EditState::Selecting {
        start_tick: 0.0,
        start_key: 0,
        current_tick: 0.0,
        current_key: 0,
        start_y: 0.0,
        current_y: 0.0,
    };
    let resizing = EditState::ResizingStart {
        note_index: 0,
        original_tick: 0.0,
        original_length: 100.0,
    };

    assert_eq!(
        ghost_delta_for_index(0, &pending, &selecting),
        Some((10, 1))
    );
    assert_eq!(ghost_delta_for_index(0, &pending, &resizing), Some((10, 1)));
    // 未在 pending 选中集合中的音符无 delta
    assert_eq!(ghost_delta_for_index(1, &pending, &selecting), None);
}

#[test]
fn ghost_delta_saturates_on_overflow() {
    let pending = Some(drag_state_with_selected(&[0], 1, i64::MAX, i16::MAX));
    let mut drag_state = drag_state_with_selected(&[0], 1, i64::MAX, i16::MAX);
    // 单独设置 delta 为 MAX，避免构造时相加溢出
    drag_state.set_delta(i64::MAX, i16::MAX);
    let edit_state = EditState::DraggingSelection { drag_state };

    assert_eq!(
        ghost_delta_for_index(0, &pending, &edit_state),
        Some((i64::MAX, i16::MAX))
    );
}
