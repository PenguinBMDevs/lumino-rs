//! 事件浏览器状态 — 对应 yinhe `right_panel/event_browser/state.rs:172`
//!
//! `SelectedItem::Automation` 统一覆盖 CC / PitchBend / RPN / NRPN / Tempo，
//! 通过 `AutomationTarget` 区分具体类型，避免为每种自动化写单独变体。
//! iced 桩保留全部变体以保证 API 兼容，状态改为纯数据（不依赖 `egui::Id` / `memory`）。

use std::collections::HashSet;

/// 事件浏览器表格行点击时产生的跳转请求（对齐 yinhe `JumpRequest`）
///
/// `note: Some((track, key))` 时跳转并闪烁音符；`None` 时仅移动播放头。
#[derive(Debug, Clone)]
pub struct JumpRequest {
    pub tick: u32,
    pub note: Option<(u16, u8)>,
}

/// 事件浏览器状态（对齐 yinhe `EventBrowserState`）
///
/// `split_ratio` 为上下分割比例（tree / detail），`fingerprint` 用于
/// `revision` 变化时增量刷新展开状态与选中越界保护。
#[derive(Debug, Clone)]
pub struct EventBrowserState {
    pub expanded_keys: HashSet<ArchiveKey>,
    pub selected_item: Option<SelectedItem>,
    pub selected_track: Option<u16>,
    pub event_page: usize,
    pub selected_ticks: HashSet<u32>,
    pub last_clicked_tick: Option<u32>,
    pub fingerprint: Option<u64>,
    pub split_ratio: f32,
}

impl Default for EventBrowserState {
    fn default() -> Self {
        Self {
            expanded_keys: HashSet::new(),
            selected_item: None,
            selected_track: None,
            event_page: 0,
            selected_ticks: HashSet::new(),
            last_clicked_tick: None,
            fingerprint: None,
            split_ratio: 0.45,
        }
    }
}

/// 事件浏览器中选中的条目（对齐 yinhe `SelectedItem` 全量变体）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SelectedItem {
    ProjectJson,
    MappingJson,
    TimeSig,
    KeySig,
    Markers,
    ConductorLyrics,
    ConductorChord,
    Notes {
        track: u16,
    },
    ProgramChange {
        track: u16,
    },
    Automation {
        track: u16,
        target: AutomationTarget,
    },
    Lyrics {
        track: u16,
    },
    Chord {
        track: u16,
    },
}

/// 归档树展开键（对齐 yinhe `ArchiveKey`）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArchiveKey {
    Conductor,
    Port(u8),
    Channel(u8, u8),
    Track(u16),
}

/// 自动化目标（对齐 yinhe `AutomationTarget` 精简）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AutomationTarget {
    Cc { controller: u8 },
    PitchBend,
    Rpn { msb: u8, lsb: u8 },
    Nrpn { msb: u8, lsb: u8 },
    Tempo,
}

impl AutomationTarget {
    #[must_use]
    pub fn display_name(&self) -> String {
        match self {
            Self::Cc { controller } => format!("CC {controller}"),
            Self::PitchBend => "PitchBend".to_string(),
            Self::Rpn { msb, lsb } => format!("RPN {msb}:{lsb}"),
            Self::Nrpn { msb, lsb } => format!("NRPN {msb}:{lsb}"),
            Self::Tempo => "Tempo".to_string(),
        }
    }

    #[must_use]
    pub fn max_value(&self) -> f32 {
        match self {
            Self::Tempo => 300.0,
            Self::PitchBend => 16383.0,
            _ => 127.0,
        }
    }
}

/// 曲线形貌（对齐 `yinhe_types::SegmentShape`）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegmentShape {
    Step,
    Curve { x1: f32, y1: f32, x2: f32, y2: f32 },
}

impl SegmentShape {
    #[must_use]
    pub fn is_linear(self) -> bool {
        matches!(self, Self::Curve { x1, y1, x2, y2 } if x1 == 0.0 && y1 == 0.0 && x2 == 0.0 && y2 == 0.0)
    }

    #[must_use]
    pub fn linear_curve() -> Self {
        Self::Curve {
            x1: 0.0,
            y1: 0.0,
            x2: 0.0,
            y2: 0.0,
        }
    }
}

/// 音符引用（对齐 yinhe `NoteRef`）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteRef {
    pub id: u32,
    pub start_tick: u32,
    pub end_tick: u32,
    pub key: u8,
    pub velocity: u8,
    pub track: u16,
}

/// 右键编辑请求（对齐 yinhe `EditRequest` 全量）
///
/// iced 桩以 `Message` 携带，而非 `egui::Id::new((salt, "edit"))` 的 memory key。
#[derive(Debug, Clone, PartialEq)]
pub enum EditRequest {
    AutoTick { tick: u32, value: f32 },
    AutoValue { tick: u32, value: f32 },
    AutoShape { tick: u32, shape: SegmentShape },
    NoteStartTick { note: NoteRef },
    NoteEndTick { note: NoteRef },
    NoteGate { note: NoteRef },
    NoteKey { note: NoteRef },
    NoteVelocity { note: NoteRef },
    TimeSigTick { tick: u32 },
    TimeSigNumerator { tick: u32 },
    TimeSigDenominator { tick: u32 },
    KeySigTick { tick: u32 },
    KeySigRoot { tick: u32 },
    KeySigScale { tick: u32 },
    PcTick { tick: u32 },
    PcProgram { tick: u32 },
    TextEventTick { kind: TextEventKind, tick: u32 },
    TextEventText { kind: TextEventKind, tick: u32 },
    DeleteSelected,
    InsertAbove { tick: u32 },
    InsertBelow { tick: u32 },
    InsertFirst,
}

/// 文本类事件种类（对齐 yinhe `TextEventKind`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextEventKind {
    Marker,
    ConductorLyrics,
    ConductorChord,
    Lyrics { track: u16 },
    Chord { track: u16 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_browser_state_default() {
        let s = EventBrowserState::default();
        assert_eq!(s.split_ratio, 0.45);
        assert!(s.selected_item.is_none());
    }

    #[test]
    fn toggle_archive_key() {
        let mut s = EventBrowserState::default();
        s.expanded_keys.insert(ArchiveKey::Conductor);
        assert!(s.expanded_keys.contains(&ArchiveKey::Conductor));
        s.expanded_keys.remove(&ArchiveKey::Conductor);
        assert!(!s.expanded_keys.contains(&ArchiveKey::Conductor));
    }

    #[test]
    fn automation_target_display() {
        assert_eq!(
            AutomationTarget::Cc { controller: 7 }.display_name(),
            "CC 7"
        );
        assert_eq!(AutomationTarget::Tempo.display_name(), "Tempo");
    }
}
