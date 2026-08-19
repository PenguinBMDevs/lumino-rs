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
    /// 空闲状态，无进行中的交互
    #[default]
    Idle,
    /// 框选（拖出选择框）状态
    Selecting {
        /// 选择框起始 tick
        start_tick: f32,
        /// 选择框起始 key
        start_key: u16,
        /// 选择框当前 tick
        current_tick: f32,
        /// 选择框当前 key
        current_key: u16,
        /// 鼠标起始 Y 像素坐标（直接跟随鼠标，不吸附到键盘格）
        start_y: f32,
        /// 鼠标当前 Y 像素坐标（直接跟随鼠标，不吸附到键盘格）
        current_y: f32,
    },
    /// 绘制（使用画笔工具）状态
    Drawing {
        /// 绘制起始 tick
        start_tick: f32,
        /// 绘制音符的 key
        key: u16,
        /// 当前 tick
        current_tick: f32,
    },
    /// 已按下但尚未确认的拖动（等待拖拽阈值）
    PendingDrag {
        /// 待拖动音符索引
        note_index: usize,
        /// 拖动起始位置（tick, key）
        start_pos: (f32, f32),
        /// 音符原始 tick
        original_tick: f32,
        /// 音符原始 key
        original_key: u16,
    },
    /// 单音符拖动（ghost 方案）
    ///
    /// 拖动期间 `EditorData.notes` 不变，仅维护 `drag_state` 偏移。
    /// 渲染时用 `ghost_position = (note.tick + delta_tick, note.key + delta_key)` 计算预览位置。
    /// `note_index` 用于音频播放与渲染高亮；`last_played_key` 用于按键变化时触发新音。
    Dragging {
        /// 被拖动音符的索引
        note_index: usize,
        /// 拖动状态（偏移量）
        drag_state: DragState,
        /// 最近一次触发发音的 key，用于按键变化时触发新音
        last_played_key: u16,
    },
    /// 调整音符起始点状态
    ResizingStart {
        /// 被调整音符索引
        note_index: usize,
        /// 音符原始起始 tick
        original_tick: f32,
        /// 音符原始长度
        original_length: f32,
    },
    /// 调整音符结束点状态
    ResizingEnd {
        /// 被调整音符索引
        note_index: usize,
        /// 音符原始长度
        original_length: f32,
    },
    /// 多音符批量拖动（ghost 方案）
    ///
    /// 拖动期间 `EditorData.notes` 不变，仅维护 `drag_state` 偏移。
    /// 选中集合以 `BitVec` 形式存于 `drag_state.selected`，与 `InteractionState.selected_notes` 保持同步。
    DraggingSelection {
        /// 拖动状态（含选中集合）
        drag_state: DragState,
    },
    /// 多音符批量复制拖动（Ctrl+拖动，ghost 方案）
    ///
    /// 与 `DraggingSelection` 的区别：原始音符保持原位不动，
    /// 副本在 `note + drag_state.delta` 位置渲染预览（UI 层）。
    /// 松手时**立即** `batch_insert_notes` 写入内存层（松手即提交，
    /// 副本真实化；连续复制从副本框继续 Ctrl+拖动）。
    DraggingSelectionCopy {
        /// 拖动状态（含选中集合）
        drag_state: DragState,
    },
    /// 批量调整选中音符起始点状态
    ResizingSelectionStart {
        /// 按下时鼠标 tick 位置（用于计算总拉伸量）
        origin_tick: f32,
        /// 上次鼠标 tick 位置
        last_tick: f32,
    },
    /// 批量调整选中音符结束点状态
    ResizingSelectionEnd {
        /// 按下时鼠标 tick 位置（用于计算总拉伸量）
        origin_tick: f32,
        /// 上次鼠标 tick 位置
        last_tick: f32,
    },
    /// 拖动 scrub 状态（点击时间轴拖动预览播放位置）
    Scrubbing,
}

/// 点击命中类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitType {
    /// 音符起始点
    Start,
    /// 音符中间位置
    Middle,
    /// 音符结束点
    End,
}

/// 选择框命中类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectionHitType {
    /// 位于选择框内部
    Inside,
    /// 位于选择框左边缘
    LeftEdge,
    /// 位于选择框右边缘
    RightEdge,
}

/// 交互状态
#[derive(Debug, Default)]
pub struct InteractionState {
    /// 当前编辑状态
    pub edit_state: EditState,
    /// 悬停目标（音符索引与命中类型）
    pub hover_state: Option<(usize, HitType)>,
    /// 当前选中音符索引集合
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
            self.pending_audio_actions.push(AudioAction::PlayNote {
                key: note.key,
                velocity: note.velocity,
            });
        }
    }
}

#[cfg(test)]
mod tests;
