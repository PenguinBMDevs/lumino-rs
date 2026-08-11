//! rendering 模块单元测试
//!
//! 从 rendering.rs 主文件拆出，因为主文件接近 400 行限制。

use super::ghost::{
    copy_delta_for_index, ghost_delta_for_index, has_active_ghost_delta, is_copy_ghosted,
    is_note_ghosted,
};
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

// ===== 复制模式（DraggingSelectionCopy / pending_copy）ghost 测试 =====

#[test]
fn copy_ghost_idle_with_pending_copy_marks_selected() {
    let pending_copy = Some(drag_state_with_selected(&[1, 3], 4, 120, 3));
    let edit_state = EditState::Idle;

    // 选中音符标记为复制 ghost（原位置 + 副本位置都要渲染）
    assert!(is_copy_ghosted(1, &pending_copy, &edit_state));
    assert!(is_copy_ghosted(3, &pending_copy, &edit_state));
    assert!(!is_copy_ghosted(0, &pending_copy, &edit_state));
    // 副本偏移来自 pending_copy
    assert_eq!(
        copy_delta_for_index(1, &pending_copy, &edit_state),
        Some((120, 3))
    );
    assert_eq!(copy_delta_for_index(0, &pending_copy, &edit_state), None);
}

#[test]
fn copy_ghost_dragging_copy_state_marks_selected() {
    let drag_state = drag_state_with_selected(&[2], 4, 240, -5);
    let edit_state = EditState::DraggingSelectionCopy { drag_state };
    let pending_copy = None;

    assert!(is_copy_ghosted(2, &pending_copy, &edit_state));
    assert!(!is_copy_ghosted(0, &pending_copy, &edit_state));
    assert_eq!(
        copy_delta_for_index(2, &pending_copy, &edit_state),
        Some((240, -5))
    );
}

#[test]
fn copy_ghost_merges_pending_copy_and_current_drag() {
    // 再次复制拖动期间：旧副本（pending_copy）与新副本（drag_state）同时可见
    let pending_copy = Some(drag_state_with_selected(&[1], 4, 100, 2));
    let drag_state = drag_state_with_selected(&[1], 4, 50, 3);
    let edit_state = EditState::DraggingSelectionCopy { drag_state };

    assert_eq!(
        copy_delta_for_index(1, &pending_copy, &edit_state),
        Some((150, 5))
    );
    assert_eq!(copy_delta_for_index(0, &pending_copy, &edit_state), None);
}

#[test]
fn copy_ghost_does_not_affect_move_ghost_semantics() {
    // 复制模式与移动模式互斥：复制中音符不被「移走」（is_note_ghosted = false），
    // 原始位置保留渲染；而普通移动中音符被移走（is_note_ghosted = true）
    let pending_copy = Some(drag_state_with_selected(&[0], 2, 100, 0));
    let copy_state = EditState::DraggingSelectionCopy {
        drag_state: drag_state_with_selected(&[0], 2, 200, 0),
    };
    assert!(
        !is_note_ghosted(0, &None, &copy_state),
        "复制中原始音符不移动"
    );

    let move_state = EditState::DraggingSelection {
        drag_state: drag_state_with_selected(&[0], 2, 200, 0),
    };
    assert!(is_note_ghosted(0, &None, &move_state), "移动中音符被移走");
    assert!(is_copy_ghosted(0, &pending_copy, &copy_state));
}

#[test]
fn has_active_ghost_delta_includes_copy_states() {
    let pending_copy = Some(drag_state_with_selected(&[0], 2, 50, 0));
    let copy_state = EditState::DraggingSelectionCopy {
        drag_state: drag_state_with_selected(&[0], 2, 60, 0),
    };
    // pending_copy 存在 → 需要 ghost 路径（副本渲染）
    assert!(has_active_ghost_delta(
        &None,
        &pending_copy,
        &EditState::Idle
    ));
    // DraggingSelectionCopy 状态 → 需要 ghost 路径
    assert!(has_active_ghost_delta(&None, &None, &copy_state));
    // 无任何 ghost 状态 → false
    assert!(!has_active_ghost_delta(&None, &None, &EditState::Idle));
}
