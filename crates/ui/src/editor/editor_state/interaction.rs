//! 交互状态管理

use crate::message::AudioAction;
use iced_core::Point;
use std::collections::HashSet;

/// 编辑状态
#[derive(Debug, Clone, Default, PartialEq)]
pub enum EditState {
    #[default]
    Idle,
    /// 框选状态
    Selecting {
        start_pos: Point,
        current_pos: Point,
    },
    Drawing {
        start_tick: f32,
        key: u16,
        current_tick: f32,
    },
    /// 预备拖动状态：点击音符后等待判断是点击还是拖动
    PendingDrag {
        note_index: usize,
        start_pos: Point,
        original_tick: f32,
        original_key: u16,
    },
    Dragging {
        note_index: usize,
        offset_tick: f32,
        offset_key: i32,
        last_played_key: u16, // 上一次播放的音高，用于避免重复播放
        original_tick: f32,
        original_key: u16,
    },
    ResizingStart {
        note_index: usize,
        original_tick: f32,
        original_length: f32,
    },
    ResizingEnd {
        note_index: usize,
    },
    /// 擦洗状态：在时间轴上拖动来快速定位播放位置
    Scrubbing,
}

/// 点击命中类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitType {
    Start,
    Middle,
    End,
}

/// 交互状态（编辑状态、悬停、选中）
#[derive(Debug, Default)]
pub struct InteractionState {
    /// 当前编辑状态
    pub edit_state: EditState,
    /// 悬停状态（音符索引, 命中类型）
    pub hover_state: Option<(usize, HitType)>,
    /// 选中的音符索引集合
    pub selected_notes: HashSet<usize>,
    /// 待处理的音频动作
    pub pending_audio_actions: Vec<AudioAction>,
}

impl InteractionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        std::mem::take(&mut self.pending_audio_actions)
    }

    /// 添加音频动作
    pub fn push_audio_action(&mut self, action: AudioAction) {
        self.pending_audio_actions.push(action);
    }
}
