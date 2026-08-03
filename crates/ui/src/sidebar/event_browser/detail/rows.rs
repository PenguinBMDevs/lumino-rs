//! 事件浏览器表格行聚合 — 元数据类事件。
//!
//! 提供拍号、调号、标记/歌词/和弦、音色变换及 project/mapping 概览
//! 的行数据收集函数。

use lumino_extras::i18n::MainTranslations;
use lumino_note_core::event::{ChordEvent, LyricsEvent, MarkerEvent, ScaleType};
use lumino_ui_core::sidebar_event::{EditRequest, TextEventKind};

use crate::sidebar::event_browser::bar_lookup::BarLookup;
use crate::sidebar::event_browser::detail::{EventBrowserData, EventTableRow, make_jump};

/// 收集拍号事件行。
pub(super) fn collect_time_sig_rows(
    data: &EventBrowserData<'_>,
    bl: &BarLookup,
    _t: &MainTranslations,
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
    t: &MainTranslations,
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
                scale_text(evt.scale, t),
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
    /// 事件所属音轨。`None` 表示全局事件（Marker），不做 track 过滤。
    fn track(&self) -> Option<u16> {
        None
    }
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
    fn track(&self) -> Option<u16> {
        Some(self.track)
    }
}

impl TextEventSource for ChordEvent {
    fn tick(&self) -> u32 {
        self.tick
    }
    fn track(&self) -> Option<u16> {
        Some(self.track)
    }
}

/// 收集文本类事件（Marker / Lyrics / Chord）行。
///
/// `track_filter`：`Some(t)` 时仅收集 `t` 音轨的事件；`None` 收集全部
/// （用于 Marker 等全局事件）。
pub(super) fn collect_text_rows<T: TextEventSource>(
    bl: &BarLookup,
    kind: TextEventKind,
    events: &[T],
    track_filter: Option<u16>,
    text_of: impl Fn(&T) -> String,
) -> Vec<EventTableRow> {
    events
        .iter()
        .filter(|evt| match track_filter {
            Some(t) => evt.track() == Some(t),
            None => true,
        })
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
    track: u16,
) -> Vec<EventTableRow> {
    data.program_changes
        .iter()
        .filter(|evt| evt.track == track)
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

/// project.json 概览行（从 EventBrowserData 读取真实元数据）。
pub(super) fn collect_project_rows(
    data: &EventBrowserData<'_>,
    t: &MainTranslations,
) -> Vec<EventTableRow> {
    let bpm = if data.tempo_points.is_empty() {
        data.project_bpm
    } else {
        data.tempo_points[0].bpm
    };
    vec![
        make_kv_row(0, t.eb_name, data.project_name),
        make_kv_row(1, t.eb_format, &format!("v{}", data.project_format_version)),
        make_kv_row(2, t.eb_author, data.project_author),
        make_kv_row(3, t.eb_bpm, &format!("{:.2}", bpm)),
        make_kv_row(4, t.eb_division, &data.project_division.to_string()),
        make_kv_row(5, t.eb_track_count, &data.project_track_count.to_string()),
        make_kv_row(6, t.eb_note_count, &data.project_note_count.to_string()),
        make_kv_row(7, t.eb_created, data.project_created),
        make_kv_row(8, t.eb_modified, data.project_modified),
    ]
}

/// mapping.json 概览行（从 EventBrowserData 读取音轨映射信息）。
pub(super) fn collect_mapping_rows(
    data: &EventBrowserData<'_>,
    t: &MainTranslations,
) -> Vec<EventTableRow> {
    let track_count = data.project_track_count;
    let note_count = data.project_note_count;
    vec![
        make_kv_row(0, t.eb_loaded, &format!("{} {}", t.eb_loaded, "")),
        make_kv_row(1, t.eb_track_count, &track_count.to_string()),
        make_kv_row(2, t.eb_note_count, &note_count.to_string()),
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
pub(super) fn scale_text(scale: ScaleType, t: &MainTranslations) -> String {
    match scale {
        ScaleType::Major => t.eb_major.to_string(),
        ScaleType::Minor => t.eb_minor.to_string(),
        ScaleType::Dorian => t.eb_dorian.to_string(),
        ScaleType::Phrygian => t.eb_phrygian.to_string(),
        ScaleType::Lydian => t.eb_lydian.to_string(),
        ScaleType::Mixolydian => t.eb_mixolydian.to_string(),
        ScaleType::Aeolian => t.eb_aeolian.to_string(),
        ScaleType::Locrian => t.eb_locrian.to_string(),
        ScaleType::HarmonicMinor => t.eb_harmonic_minor.to_string(),
        ScaleType::MelodicMinor => t.eb_melodic_minor.to_string(),
    }
}
