//! 事件浏览器表格行聚合 — 元数据类事件。
//!
//! 提供拍号、调号、标记/歌词/和弦、音色变换及 project/mapping 概览
//! 的行数据收集函数。

use lumino_note_core::event::{ChordEvent, LyricsEvent, MarkerEvent, ScaleType};
use lumino_ui_core::sidebar_event::{EditRequest, TextEventKind};

use crate::sidebar::event_browser::bar_lookup::BarLookup;
use crate::sidebar::event_browser::detail::{EventBrowserData, EventTableRow, make_jump};

/// 收集拍号事件行。
pub(super) fn collect_time_sig_rows(
    data: &EventBrowserData<'_>,
    bl: &BarLookup,
) -> Vec<EventTableRow> {
    data.time_signatures
        .iter()
        .enumerate()
        .map(|(idx, (tick, num, den))| {
            let tick = *tick;
            let cells = vec![
                String::new(), // # 在排序后回填
                tick.to_string(),
                bl.format(tick),
                num.to_string(),
                den.to_string(),
            ];
            let edits = vec![
                None,
                Some(EditRequest::TimeSigTick { tick }),
                Some(EditRequest::TimeSigTick { tick }),
                Some(EditRequest::TimeSigNumerator { tick }),
                Some(EditRequest::TimeSigDenominator { tick }),
            ];
            let jumps = vec![
                None,
                make_jump(tick, None),
                make_jump(tick, None),
                make_jump(tick, None),
                make_jump(tick, None),
            ];
            EventTableRow {
                id: idx,
                tick,
                cells,
                cell_edits: edits,
                cell_jumps: jumps,
            }
        })
        .collect()
}

/// 收集调号事件行。
pub(super) fn collect_key_sig_rows(
    data: &EventBrowserData<'_>,
    bl: &BarLookup,
) -> Vec<EventTableRow> {
    data.key_signatures
        .iter()
        .enumerate()
        .map(|(idx, evt)| {
            let tick = evt.tick;
            let cells = vec![
                String::new(),
                tick.to_string(),
                bl.format(tick),
                evt.root.to_string(),
                scale_text(evt.scale),
            ];
            let edits = vec![
                None,
                Some(EditRequest::KeySigTick { tick }),
                Some(EditRequest::KeySigTick { tick }),
                Some(EditRequest::KeySigRoot { tick }),
                Some(EditRequest::KeySigScale { tick }),
            ];
            let jumps = vec![
                None,
                make_jump(tick, None),
                make_jump(tick, None),
                make_jump(tick, None),
                make_jump(tick, None),
            ];
            EventTableRow {
                id: idx,
                tick,
                cells,
                cell_edits: edits,
                cell_jumps: jumps,
            }
        })
        .collect()
}

/// 文本类事件统一 trait：提供 tick 访问。
pub(super) trait TextEventSource {
    fn tick(&self) -> u32;
}

impl TextEventSource for MarkerEvent {
    fn tick(&self) -> u32 {
        self.tick
    }
}

impl TextEventSource for LyricsEvent {
    fn tick(&self) -> u32 {
        self.tick
    }
}

impl TextEventSource for ChordEvent {
    fn tick(&self) -> u32 {
        self.tick
    }
}

/// 收集文本类事件（Marker / Lyrics / Chord）行。
pub(super) fn collect_text_rows<T: TextEventSource>(
    bl: &BarLookup,
    kind: TextEventKind,
    events: &[T],
    text_of: impl Fn(&T) -> String,
) -> Vec<EventTableRow> {
    events
        .iter()
        .enumerate()
        .map(|(idx, evt)| {
            let tick = evt.tick();
            let cells = vec![
                String::new(),
                tick.to_string(),
                bl.format(tick),
                text_of(evt),
            ];
            let edits = vec![
                None,
                Some(EditRequest::TextEventTick { kind, tick }),
                Some(EditRequest::TextEventTick { kind, tick }),
                Some(EditRequest::TextEventText { kind, tick }),
            ];
            let jumps = vec![
                None,
                make_jump(tick, None),
                make_jump(tick, None),
                make_jump(tick, None),
            ];
            EventTableRow {
                id: idx,
                tick,
                cells,
                cell_edits: edits,
                cell_jumps: jumps,
            }
        })
        .collect()
}

/// 收集音色变换事件行。
pub(super) fn collect_pc_rows(
    data: &EventBrowserData<'_>,
    bl: &BarLookup,
    _track: u16,
) -> Vec<EventTableRow> {
    data.program_changes
        .iter()
        .enumerate()
        .map(|(idx, evt)| {
            let tick = evt.tick;
            let cells = vec![
                String::new(),
                tick.to_string(),
                bl.format(tick),
                evt.program.to_string(),
            ];
            let edits = vec![
                None,
                Some(EditRequest::PcTick { tick }),
                Some(EditRequest::PcTick { tick }),
                Some(EditRequest::PcProgram { tick }),
            ];
            let jumps = vec![
                None,
                make_jump(tick, None),
                make_jump(tick, None),
                make_jump(tick, None),
            ];
            EventTableRow {
                id: idx,
                tick,
                cells,
                cell_edits: edits,
                cell_jumps: jumps,
            }
        })
        .collect()
}

/// project.json 概览行（静态 key-value）。
pub(super) fn collect_project_rows() -> Vec<EventTableRow> {
    vec![
        make_kv_row(0, "project.json", "loaded"),
        make_kv_row(1, "format", "v1"),
    ]
}

/// mapping.json 概览行（静态 key-value）。
pub(super) fn collect_mapping_rows() -> Vec<EventTableRow> {
    vec![
        make_kv_row(0, "mapping.json", "loaded"),
        make_kv_row(1, "version", "1"),
    ]
}

fn make_kv_row(id: usize, key: &str, value: &str) -> EventTableRow {
    EventTableRow {
        id,
        tick: 0,
        cells: vec![key.to_string(), value.to_string()],
        cell_edits: vec![None, None],
        cell_jumps: vec![None, None],
    }
}

/// 调式显示名称。
pub(super) fn scale_text(scale: ScaleType) -> String {
    match scale {
        ScaleType::Major => "Major".to_string(),
        ScaleType::Minor => "Minor".to_string(),
        ScaleType::Dorian => "Dorian".to_string(),
        ScaleType::Phrygian => "Phrygian".to_string(),
        ScaleType::Lydian => "Lydian".to_string(),
        ScaleType::Mixolydian => "Mixolydian".to_string(),
        ScaleType::Aeolian => "Aeolian".to_string(),
        ScaleType::Locrian => "Locrian".to_string(),
        ScaleType::HarmonicMinor => "HarmonicMinor".to_string(),
        ScaleType::MelodicMinor => "MelodicMinor".to_string(),
    }
}
