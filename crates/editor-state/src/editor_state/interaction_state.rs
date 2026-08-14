//! 交互状态机

use std::collections::{HashSet, VecDeque};
use std::time::Instant;

use lumino_core::AudioAction;
use lumino_note_core::note_store::BitSet;

use crate::editor_state::drag_state::DragState;

/// 批量拖动预览序列中的单个音符。
///
/// `play_at` 是该音符的绝对播放时刻（按工程 BPM 与 tick 间隔换算），
/// `drain_preview_sequence` 在 `play_at` 到达时才把音符弹入音频动作队列。
#[derive(Debug, Clone, Copy)]
pub struct PreviewSequenceNote {
    /// 绝对播放时刻（tick 间隔 × BPM 换算）
    pub play_at: Instant,
    /// 音符 key（拖动后的 ghost 位置）
    pub key: u8,
    /// 播放力度
    pub velocity: u8,
}

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
        /// 鼠标起始 Y 像素坐标（直接跟随鼠标，不吸附到键盘格）
        start_y: f32,
        /// 鼠标当前 Y 像素坐标（直接跟随鼠标，不吸附到键盘格）
        current_y: f32,
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
    /// 多音符批量复制拖动（Ctrl+拖动，ghost 方案）
    ///
    /// 与 `DraggingSelection` 的区别：原始音符保持原位不动，
    /// 副本在 `note + drag_state.delta` 位置渲染预览（UI 层）。
    /// 松手时**立即** `batch_insert_notes` 写入内存层（松手即提交，
    /// 副本真实化；连续复制从副本框继续 Ctrl+拖动）。
    DraggingSelectionCopy {
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
    /// 基于位向量的选中集合，用于高效表示"全选"或"大部分选中"。
    ///
    /// 当 `Some` 时，`selected_notes` 被忽略，选中状态由 `selection_bitset` 决定。
    /// 当 `None` 时，选中状态由 `selected_notes` 决定。
    ///
    /// 用于 `select_all_notes` 热路径，避免创建 16M 条目的 `HashSet`（512MB 表 + 16M SipHash 插入）。
    /// `BitSet` 16M 位仅 256KB，支持 O(1) 位测试和 O(K) trailing_zeros 遍历。
    pub selection_bitset: Option<BitSet>,
    /// 待处理的音频动作
    pub pending_audio_actions: Vec<AudioAction>,
    /// 批量拖动预览序列：按工程 BPM 时序排列的待播放音符。
    ///
    /// 批量拖动（`DraggingSelection` / `DraggingSelectionCopy`）中 key 上下移动时，
    /// 由交互层按选中音符的 tick 顺序 + 当前 ghost key 位置 + BPM 换算的
    /// 播放时刻构建该序列，`drain_preview_sequence` 在各自 `play_at` 时刻
    /// 到达时逐个弹出发声——真实时序预览，而非固定间隔琶音。
    pub preview_sequence: VecDeque<PreviewSequenceNote>,
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

    /// 替换为新的批量拖动预览序列。
    ///
    /// 覆盖旧的未播完序列。条目必须按 `play_at` 升序排列
    /// （由调用方按 tick 顺序 + BPM 换算生成，首个条目 `play_at` 为当前时刻）。
    pub fn set_preview_sequence(&mut self, notes: Vec<PreviewSequenceNote>) {
        self.preview_sequence.clear();
        self.preview_sequence.extend(notes);
    }

    /// 清空批量拖动预览序列（拖动回到原位 / 松手时调用）
    pub fn clear_preview_sequence(&mut self) {
        self.preview_sequence.clear();
    }

    /// 弹出所有到达播放时刻的预览序列音符到待处理音频动作。
    ///
    /// 以 `now` 为当前时刻（调用方注入，便于测试）：`play_at <= now` 的
    /// 条目全部弹出（同帧内到期的音符合并弹出，保证按 BPM 真实时序发声）。
    pub fn drain_preview_sequence(&mut self, now: Instant) {
        while let Some(note) = self.preview_sequence.front().copied() {
            if note.play_at > now {
                break;
            }
            self.preview_sequence.pop_front();
            self.pending_audio_actions
                .push(AudioAction::PlayNote { key: note.key, velocity: note.velocity });
        }
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
    fn test_dragging_selection_copy_variant() {
        use crate::editor_state::drag_state::DragState;
        // 复制拖动变体携带独立 DragState，可区分于移动拖动
        let copy_state = EditState::DraggingSelectionCopy {
            drag_state: DragState::default(),
        };
        let move_state = EditState::DraggingSelection {
            drag_state: DragState::default(),
        };
        assert_ne!(copy_state, move_state);
        assert_ne!(copy_state, EditState::default());
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

    #[test]
    fn test_set_preview_sequence_replaces_old() {
        let mut state = InteractionState::default();
        let now = Instant::now();
        let note = |play_at: Instant, key: u8| PreviewSequenceNote {
            play_at,
            key,
            velocity: 100,
        };
        state.set_preview_sequence(vec![note(now, 60), note(now, 62)]);
        // 替换旧序列：旧序列被清空
        state.set_preview_sequence(vec![note(now, 64)]);
        assert_eq!(state.preview_sequence.len(), 1);
        assert_eq!(state.preview_sequence[0].key, 64);
    }

    #[test]
    fn test_drain_preview_sequence_by_play_time() {
        let mut state = InteractionState::default();
        let t0 = Instant::now();
        // 按 BPM 时序：0ms、500ms、1000ms 各一个音符
        state.set_preview_sequence(vec![
            PreviewSequenceNote {
                play_at: t0,
                key: 60,
                velocity: 100,
            },
            PreviewSequenceNote {
                play_at: t0 + std::time::Duration::from_millis(500),
                key: 62,
                velocity: 100,
            },
            PreviewSequenceNote {
                play_at: t0 + std::time::Duration::from_millis(1000),
                key: 64,
                velocity: 100,
            },
        ]);

        // t=0：第一个音符立即弹出
        assert_eq!(drain_at(&mut state, t0), Some(60));
        // t=100ms：第二个还没到
        assert_eq!(
            drain_at(&mut state, t0 + std::time::Duration::from_millis(100)),
            None,
            "未到 play_at 的音符不应弹出"
        );
        // t=500ms：第二个到达
        assert_eq!(
            drain_at(&mut state, t0 + std::time::Duration::from_millis(500)),
            Some(62)
        );
        // t=900ms：第三个还没到
        assert_eq!(
            drain_at(&mut state, t0 + std::time::Duration::from_millis(900)),
            None
        );
        // t=1000ms：第三个到达，且同帧到期的可合并弹出
        assert_eq!(
            drain_at(&mut state, t0 + std::time::Duration::from_millis(1000)),
            Some(64)
        );
        assert!(state.preview_sequence.is_empty(), "序列应播放完毕");
    }

    #[test]
    fn test_drain_preview_sequence_merges_due_notes() {
        let mut state = InteractionState::default();
        let t0 = Instant::now();
        // 两个音符同一时刻到期：一次 drain 应全部弹出（保持正确时序，不丢帧）
        state.set_preview_sequence(vec![
            PreviewSequenceNote {
                play_at: t0,
                key: 60,
                velocity: 100,
            },
            PreviewSequenceNote {
                play_at: t0 + std::time::Duration::from_millis(100),
                key: 62,
                velocity: 100,
            },
        ]);
        let now = t0 + std::time::Duration::from_millis(500);
        state.drain_preview_sequence(now);
        let keys: Vec<u8> = state
            .pending_audio_actions
            .iter()
            .filter_map(|a| match a {
                AudioAction::PlayNote { key, .. } => Some(*key),
                _ => None,
            })
            .collect();
        assert_eq!(keys, vec![60, 62], "到期的音符应按序全部弹出");
        assert!(state.preview_sequence.is_empty());
    }

    #[test]
    fn test_drain_preview_sequence_after_clear_noop() {
        let mut state = InteractionState::default();
        let now = Instant::now();
        state.set_preview_sequence(vec![PreviewSequenceNote {
            play_at: now,
            key: 60,
            velocity: 100,
        }]);
        state.clear_preview_sequence();
        assert!(state.preview_sequence.is_empty());
        assert_eq!(drain_at(&mut state, now), None);
        assert!(state.pending_audio_actions.is_empty());
    }

    #[test]
    fn test_clear_preview_sequence_discards_pending() {
        let mut state = InteractionState::default();
        let now = Instant::now();
        state.set_preview_sequence(vec![PreviewSequenceNote {
            play_at: now,
            key: 60,
            velocity: 100,
        }]);
        // 弹出第一个后清空，再设置新序列：新序列按自身 play_at 播放
        let _ = drain_at(&mut state, now);
        state.clear_preview_sequence();
        state.set_preview_sequence(vec![PreviewSequenceNote {
            play_at: now + std::time::Duration::from_millis(200),
            key: 70,
            velocity: 100,
        }]);
        assert_eq!(drain_at(&mut state, now), None, "未到 play_at 不应弹出");
        assert_eq!(
            drain_at(&mut state, now + std::time::Duration::from_millis(200)),
            Some(70)
        );
    }

    /// 单次弹出辅助：在 `now` 时刻调用 `drain_preview_sequence`，
    /// 取出并清空音频动作，返回本次弹出的第一个音符 key（无弹出则 None）。
    fn drain_at(state: &mut InteractionState, now: Instant) -> Option<u8> {
        state.drain_preview_sequence(now);
        let action = state.pending_audio_actions.drain(..).next();
        match action {
            Some(AudioAction::PlayNote { key, .. }) => Some(key),
            _ => None,
        }
    }
}
