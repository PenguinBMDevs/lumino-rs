//! 交互业务逻辑
//!
//! 将 `EditorState` 中与交互状态、音符编辑相关的业务逻辑提取到此处，
//! 降低 facade 复杂度并提高单测可行性。

use super::constants::DEFAULT_PREVIEW_VELOCITY;
use super::editor_data::EditorData;
use super::interaction_state::{EditState, HitType, InteractionState};

/// 开始编辑现有音符
///
/// 根据命中类型设置不同的编辑状态：
/// - `Start` / `End`：调整音符起止位置，先推入历史记录
/// - `Middle`：进入拖拽 pending 状态，并播放预览音
pub fn start_note_edit(
    data: &mut EditorData,
    interaction: &mut InteractionState,
    index: usize,
    hit_type: HitType,
    pos: (f32, f32),
) {
    match hit_type {
        HitType::Start => {
            data.push_history();
            let note = &data.notes[index];
            interaction.edit_state = EditState::ResizingStart {
                note_index: index,
                original_tick: note.tick,
                original_length: note.length,
            };
        }
        HitType::End => {
            data.push_history();
            let note = &data.notes[index];
            interaction.edit_state = EditState::ResizingEnd {
                note_index: index,
                original_length: note.length,
            };
        }
        HitType::Middle => {
            let note = &data.notes[index];
            interaction.edit_state = EditState::PendingDrag {
                note_index: index,
                start_pos: pos,
                original_tick: note.tick,
                original_key: note.key,
            };
            interaction.play_note_audio(note.key, DEFAULT_PREVIEW_VELOCITY);
        }
    }
}

/// 开始绘制新音符
pub fn start_drawing(interaction: &mut InteractionState, snapped_tick: f32, key: u16) {
    interaction.edit_state = EditState::Drawing {
        start_tick: snapped_tick,
        key,
        current_tick: snapped_tick,
    };
    interaction.play_note_audio(key, DEFAULT_PREVIEW_VELOCITY);
}

/// 应用音符变化（单音符编辑），返回是否发生了变更
///
/// **ghost 方案**：`Dragging` 期间不通过此函数写入（drag 期间 data.notes 不变，
/// 松手时由 `finalize_dragging` 一次性 apply）。此函数仅服务 `ResizingStart` / `ResizingEnd`。
pub fn apply_note_changes(
    data: &mut EditorData,
    edit_state: &EditState,
    new_tick: Option<f32>,
    new_key: Option<u16>,
    new_length: Option<f32>,
) -> bool {
    let note_index = match edit_state {
        EditState::ResizingStart { note_index, .. } | EditState::ResizingEnd { note_index, .. } => {
            *note_index
        }
        // ghost 方案：Dragging/DraggingSelection 期间不通过此函数写入
        EditState::Dragging { .. }
        | EditState::DraggingSelection { .. }
        | EditState::ResizingSelectionStart { .. }
        | EditState::ResizingSelectionEnd { .. } => return false,
        _ => return false,
    };

    if let Some(note) = data.notes.get_mut(note_index) {
        let mut changed = false;
        if let Some(t) = new_tick {
            note.tick = t;
            changed = true;
        }
        if let Some(k) = new_key {
            note.key = k;
            changed = true;
        }
        if let Some(l) = new_length {
            note.length = l;
            changed = true;
        }
        return changed;
    }
    false
}

/// 处理删除键按下事件，返回被删除音符的索引
pub fn handle_delete_pressed(
    data: &mut EditorData,
    hover_state: Option<(usize, HitType)>,
) -> Option<usize> {
    if let Some((index, _)) = hover_state {
        data.delete_note_by_index(index);
        Some(index)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_state::DragState;
    use crate::note::Note;

    fn setup_state() -> (EditorData, InteractionState) {
        (EditorData::new(), InteractionState::default())
    }

    #[test]
    fn test_start_note_edit_start() {
        let (mut data, mut interaction) = setup_state();
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        start_note_edit(&mut data, &mut interaction, 0, HitType::Start, (0.0, 0.0));
        assert!(matches!(
            interaction.edit_state,
            EditState::ResizingStart { note_index: 0, .. }
        ));
    }

    #[test]
    fn test_start_note_edit_middle() {
        let (mut data, mut interaction) = setup_state();
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        start_note_edit(&mut data, &mut interaction, 0, HitType::Middle, (0.0, 0.0));
        assert!(matches!(
            interaction.edit_state,
            EditState::PendingDrag { note_index: 0, .. }
        ));
        assert_eq!(interaction.pending_audio_actions.len(), 1);
    }

    #[test]
    fn test_start_drawing() {
        let (_, mut interaction) = setup_state();
        start_drawing(&mut interaction, 10.0, 60);
        assert!(matches!(
            interaction.edit_state,
            EditState::Drawing {
                start_tick: 10.0,
                key: 60,
                ..
            }
        ));
        assert_eq!(interaction.pending_audio_actions.len(), 1);
    }

    #[test]
    fn test_apply_note_changes_dragging_is_noop() {
        // ghost 方案：Dragging 期间 apply_note_changes 不再写入 notes
        let (mut data, mut interaction) = setup_state();
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        interaction.edit_state = EditState::Dragging {
            note_index: 0,
            drag_state: DragState::from_single(0, 1, 0, 60),
            last_played_key: 60,
        };
        assert!(!apply_note_changes(
            &mut data,
            &interaction.edit_state,
            Some(2.0),
            Some(64),
            Some(3.0)
        ));
        let note = &data.notes[0];
        assert_eq!(note.tick, 0.0, "Dragging 期间 notes 不应被修改");
        assert_eq!(note.key, 60);
        assert_eq!(note.length, 1.0);
    }

    #[test]
    fn test_apply_note_changes_non_edit_state() {
        let (mut data, _interaction) = setup_state();
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        assert!(!apply_note_changes(
            &mut data,
            &EditState::Idle,
            Some(2.0),
            None,
            None
        ));
    }

    #[test]
    fn test_handle_delete_pressed() {
        let (mut data, _) = setup_state();
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        let result = handle_delete_pressed(&mut data, Some((0, HitType::Middle)));
        assert_eq!(result, Some(0));
        assert!(data.notes.is_empty());
    }

    #[test]
    fn test_handle_delete_pressed_no_hover() {
        let (mut data, _) = setup_state();
        assert!(handle_delete_pressed(&mut data, None).is_none());
    }
}
