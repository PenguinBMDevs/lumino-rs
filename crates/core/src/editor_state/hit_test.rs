//! 编辑器碰撞检测逻辑
//!
//! 将命中测试、选择框边界计算、选择框命中测试等纯几何/数据交互逻辑
//! 从 `EditorState` facade 中提取出来，降低 facade 的复杂度并提高可测试性。

use std::collections::HashSet;

use crate::note::Note;
use crate::view_state::ViewState;

use super::constants::SELECTION_BOX_EDGE_THRESHOLD;
use super::interaction_state::{HitType, SelectionHitType};

/// 检测坐标是否落在某个音符上
///
/// 从后向前遍历音符，优先匹配视觉上靠上的音符。
pub fn hit_test_note(
    notes: &im::Vector<Note>,
    view: &ViewState,
    pos: (f32, f32),
    edge_threshold_px: f32,
) -> Option<(usize, HitType)> {
    let tick = view.x_to_tick(pos.0);
    let key = view.y_to_key(pos.1);
    for (note_idx, note) in notes.iter().enumerate().rev() {
        if note.key == key && tick >= note.tick && tick <= note.tick + note.length {
            let start_delta = (tick - note.tick).abs();
            let end_delta = (tick - (note.tick + note.length)).abs();
            let edge_threshold = edge_threshold_px / view.zoom_x;
            if end_delta < edge_threshold {
                return Some((note_idx, HitType::End));
            }
            if start_delta < edge_threshold {
                return Some((note_idx, HitType::Start));
            }
            return Some((note_idx, HitType::Middle));
        }
    }
    None
}

/// 计算选中音符的边界框（像素坐标）
///
/// 返回 `(min_x, max_x, min_y, max_y)`，如果未选中任何有效音符则返回 `None`。
pub fn get_selection_box_bounds(
    notes: &im::Vector<Note>,
    view: &ViewState,
    selected_notes: &HashSet<usize>,
) -> Option<(f32, f32, f32, f32)> {
    if selected_notes.is_empty() {
        return None;
    }
    let mut min_t = f32::INFINITY;
    let mut max_te = f32::NEG_INFINITY;
    let mut max_k = u16::MIN;
    let mut min_k = u16::MAX;
    for &note_idx in selected_notes.iter() {
        if let Some(note) = notes.get(note_idx) {
            min_t = min_t.min(note.tick);
            max_te = max_te.max(note.tick + note.length);
            max_k = max_k.max(note.key);
            min_k = min_k.min(note.key);
        }
    }
    if min_t.is_infinite() {
        return None;
    }
    Some((
        view.tick_to_x(min_t),
        view.tick_to_x(max_te),
        view.key_to_y(max_k),
        view.key_to_y(min_k) + view.zoom_y,
    ))
}

/// 检测坐标相对于选择框边界的位置
///
/// 输入 `bounds` 为 `get_selection_box_bounds` 的返回值，避免在内部重复计算。
pub fn hit_test_selection_box(
    bounds: (f32, f32, f32, f32),
    pos: (f32, f32),
) -> Option<SelectionHitType> {
    let (min_x, max_x, min_y, max_y) = bounds;
    if pos.0 < min_x || pos.0 > max_x || pos.1 < min_y || pos.1 > max_y {
        return None;
    }
    let et = SELECTION_BOX_EDGE_THRESHOLD;
    let on_left = (pos.0 - min_x).abs() < et;
    let on_right = (pos.0 - max_x).abs() < et;
    if on_left && !on_right {
        return Some(SelectionHitType::LeftEdge);
    }
    if on_right && !on_left {
        return Some(SelectionHitType::RightEdge);
    }
    Some(SelectionHitType::Inside)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_state::EditorState;
    use crate::editor_state::interaction_state::HitType;
    use crate::note::Note;

    #[test]
    fn test_hit_test_note_middle() {
        let mut state = EditorState::new();
        // 使用足够长的音符，使边缘阈值不会覆盖整个音符
        state.data.notes.push_back(Note::new(0.0, 60, 400.0));
        let x = state.view.tick_to_x(200.0);
        let y = state.view.key_to_y(60);
        let hit_result = hit_test_note(&state.data.notes, &state.view, (x, y), 4.0);
        assert_eq!(hit_result, Some((0, HitType::Middle)));
    }

    #[test]
    fn test_hit_test_note_start() {
        let mut state = EditorState::new();
        state.data.notes.push_back(Note::new(0.0, 60, 400.0));
        let x = state.view.tick_to_x(10.0);
        let y = state.view.key_to_y(60);
        let hit_result = hit_test_note(&state.data.notes, &state.view, (x, y), 4.0);
        assert_eq!(hit_result, Some((0, HitType::Start)));
    }

    #[test]
    fn test_hit_test_note_end() {
        let mut state = EditorState::new();
        state.data.notes.push_back(Note::new(0.0, 60, 400.0));
        let x = state.view.tick_to_x(390.0);
        let y = state.view.key_to_y(60);
        let hit_result = hit_test_note(&state.data.notes, &state.view, (x, y), 4.0);
        assert_eq!(hit_result, Some((0, HitType::End)));
    }

    #[test]
    fn test_hit_test_note_miss() {
        let mut state = EditorState::new();
        state.data.notes.push_back(Note::new(0.0, 60, 400.0));
        let x = state.view.tick_to_x(500.0);
        let y = state.view.key_to_y(60);
        let hit_result = hit_test_note(&state.data.notes, &state.view, (x, y), 4.0);
        assert!(hit_result.is_none());
    }

    #[test]
    fn test_get_selection_box_bounds() {
        let mut state = EditorState::new();
        state.data.notes.push_back(Note::new(0.0, 60, 2.0));
        state.data.notes.push_back(Note::new(4.0, 62, 2.0));
        state.interaction.selected_notes.insert(0);
        state.interaction.selected_notes.insert(1);

        let bounds = get_selection_box_bounds(
            &state.data.notes,
            &state.view,
            &state.interaction.selected_notes,
        );
        assert!(bounds.is_some());
        let (min_x, max_x, min_y, max_y) =
            bounds.expect("选中了 2 个音符，get_selection_box_bounds 应返回 Some");
        assert!(min_x < max_x);
        assert!(min_y < max_y);
    }

    #[test]
    fn test_get_selection_box_bounds_empty() {
        let state = EditorState::new();
        let bounds = get_selection_box_bounds(
            &state.data.notes,
            &state.view,
            &state.interaction.selected_notes,
        );
        assert!(bounds.is_none());
    }

    #[test]
    fn test_hit_test_selection_box_inside() {
        let bounds = (0.0, 100.0, 0.0, 100.0);
        let hit_result = hit_test_selection_box(bounds, (50.0, 50.0));
        assert_eq!(hit_result, Some(SelectionHitType::Inside));
    }

    #[test]
    fn test_hit_test_selection_box_left_edge() {
        let bounds = (0.0, 100.0, 0.0, 100.0);
        let hit_result = hit_test_selection_box(bounds, (2.0, 50.0));
        assert_eq!(hit_result, Some(SelectionHitType::LeftEdge));
    }

    #[test]
    fn test_hit_test_selection_box_right_edge() {
        let bounds = (0.0, 100.0, 0.0, 100.0);
        let hit_result = hit_test_selection_box(bounds, (98.0, 50.0));
        assert_eq!(hit_result, Some(SelectionHitType::RightEdge));
    }

    #[test]
    fn test_hit_test_selection_box_miss() {
        let bounds = (0.0, 100.0, 0.0, 100.0);
        let hit_result = hit_test_selection_box(bounds, (200.0, 50.0));
        assert!(hit_result.is_none());
    }
}
