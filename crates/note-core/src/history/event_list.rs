//! 事件列表操作类型
//!
//! 供事件浏览器进行撤销/重做与批量操作的数据模型。

use crate::event::{
    AutomationEvent, AutomationTarget, ChordEvent, KeySignatureEvent, LyricsEvent, MarkerEvent,
    ProgramChangeEvent, TimeSignatureEvent,
};

/// 事件列表中的单个事件条目
#[derive(Debug, Clone, PartialEq)]
pub enum EventListItem {
    /// 拍号事件
    TimeSig(TimeSignatureEvent),
    /// 调号事件
    KeySig(KeySignatureEvent),
    /// 标记事件
    Marker(MarkerEvent),
    /// 歌词事件
    Lyrics(LyricsEvent),
    /// 和弦事件
    Chord(ChordEvent),
    /// 音色变换事件
    ProgramChange(ProgramChangeEvent),
    /// 自动化事件
    Automation(AutomationEvent),
}

/// 事件列表操作的目标位置
#[derive(Debug, Clone, PartialEq)]
pub enum EventListTarget {
    /// 工程 JSON
    ProjectJson,
    /// 映射 JSON
    MappingJson,
    /// 指挥轨拍号
    ConductorTimeSig,
    /// 指挥轨调号
    ConductorKeySig,
    /// 指挥轨标记
    ConductorMarkers,
    /// 指挥轨歌词
    ConductorLyrics,
    /// 指挥轨和弦
    ConductorChord,
    /// 指挥轨速度
    ConductorTempo,
    /// 普通音轨音符
    TrackNotes(u16),
    /// 普通音轨音色变换
    TrackProgramChange(u16),
    /// 普通音轨歌词
    TrackLyrics(u16),
    /// 普通音轨和弦
    TrackChord(u16),
    /// 普通音轨自动化
    TrackAutomation(u16, AutomationTarget),
}

/// 事件列表的增量快照
#[derive(Debug, Clone, PartialEq)]
pub struct EventListDelta {
    /// 操作目标
    pub target: EventListTarget,
    /// 操作前的完整事件列表
    pub old: Vec<EventListItem>,
    /// 操作后的完整事件列表
    pub new: Vec<EventListItem>,
}

impl EventListDelta {
    /// 返回反向增量
    pub fn inverse(self) -> Self {
        Self {
            target: self.target,
            old: self.new,
            new: self.old,
        }
    }
}

/// 撤销动作
#[derive(Debug, Clone, PartialEq)]
pub enum UndoAction {
    /// 事件列表操作
    EventList(EventListDelta),
}
