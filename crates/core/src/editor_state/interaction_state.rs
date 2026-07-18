//! 交互状态机

use std::collections::HashSet;

use crate::AudioAction;
use crate::editor_state::drag_state::DragState;

/// 编辑状态
#[derive(Debug, Clone, Default, PartialEq)]
pub enum EditState {
    #[default]
    Idle,
    Selecting {
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    },
    Drawing {
        start_tick: f32,
        key: u16,
        current_tick: f32,
    },
    PendingDrag {
        note_index: usize,
        start_pos: (f32, f32),
        original_tick: f32,
        original_key: u16,
    },
    /// 单音符拖动（ghost 方案）
    ///
    /// 拖动期间 `EditorData.notes` 不变，仅维护 `drag_state` 偏移。
    /// 渲染时用 `ghost_position = (note.tick + delta_tick, note.key + delta_key)` 计算预览位置。
    /// `note_index` 用于音频播放与渲染高亮；`last_played_key` 用于按键变化时触发新音。
    Dragging {
        note_index: usize,
        drag_state: DragState,
        last_played_key: u16,
    },
    ResizingStart {
        note_index: usize,
        original_tick: f32,
        original_length: f32,
    },
    ResizingEnd {
        note_index: usize,
        original_length: f32,
    },
    /// 多音符批量拖动（ghost 方案）
    ///
    /// 拖动期间 `EditorData.notes` 不变，仅维护 `drag_state` 偏移。
    /// 选中集合以 `BitVec` 形式存于 `drag_state.selected`，与 `InteractionState.selected_notes` 保持同步。
    DraggingSelection {
        drag_state: DragState,
    },
    ResizingSelectionStart {
        last_tick: f32,
    },
    ResizingSelectionEnd {
        last_tick: f32,
    },
    Scrubbing,
}

/// 点击命中类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitType {
    Start,
    Middle,
    End,
}

/// 选择框命中类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectionHitType {
    Inside,
    LeftEdge,
    RightEdge,
}

/// 交互状态
#[derive(Debug, Default)]
pub struct InteractionState {
    pub edit_state: EditState,
    pub hover_state: Option<(usize, HitType)>,
    pub selected_notes: HashSet<usize>,
    /// 待处理的音频动作
    pub pending_audio_actions: Vec<AudioAction>,
}

impl InteractionState {
    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        std::mem::take(&mut self.pending_audio_actions)
    }

    /// 添加音频动作
    pub fn push_audio_action(&mut self, action: AudioAction) {
        self.pending_audio_actions.push(action);
    }

    /// 添加播放音符的音频动作
    pub fn play_note_audio(&mut self, key: u16, velocity: u8) {
        self.pending_audio_actions.push(AudioAction::PlayNote {
            key: key as u8,
            velocity,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_state_default_is_idle() {
        assert_eq!(EditState::default(), EditState::Idle);
    }

    #[test]
    fn test_hit_type_variants() {
        assert_ne!(HitType::Start, HitType::Middle);
        assert_ne!(HitType::Middle, HitType::End);
    }

    #[test]
    fn test_selection_hit_type_variants() {
        let variants = [
            SelectionHitType::Inside,
            SelectionHitType::LeftEdge,
            SelectionHitType::RightEdge,
        ];
        for i in 0..variants.len() {
            for j in (i + 1)..variants.len() {
                assert_ne!(variants[i], variants[j]);
            }
        }
    }

    #[test]
    fn test_take_audio_actions() {
        let mut state = InteractionState::default();
        state.push_audio_action(AudioAction::PlayNote {
            key: 60,
            velocity: 100,
        });
        let actions = state.take_audio_actions();
        assert_eq!(actions.len(), 1);
        assert!(state.pending_audio_actions.is_empty());
    }

    #[test]
    fn test_play_note_audio() {
        let mut state = InteractionState::default();
        state.play_note_audio(72, 80);
        assert_eq!(state.pending_audio_actions.len(), 1);
    }
}
