//! 事件浏览器表格行数据聚合。
//!
//! 将 `SelectedItem` 与只读数据 `EventBrowserData` 转换为统一表格行，
//! 供 Canvas 绘制与交互使用。
//!
//! 各事件类型的行收集拆分为子模块：
//! - `rows`：拍号、调号、文本、音色变换、project/mapping
//! - `note`：音符事件
//! - `auto`：自动化事件（CC/PB/RPN/NRPN/Tempo）

use std::sync::Arc;

use lumino_note_core::automation::AutomationLane;
use lumino_note_core::event::{
    ChordEvent, KeySignatureEvent, LyricsEvent, MarkerEvent, ProgramChangeEvent,
};
use lumino_note_core::midi_types::TempoPoint;
use lumino_note_core::note::Note;
use lumino_ui_core::sidebar_event::{EditRequest, JumpRequest, TextEventKind};

use crate::sidebar::core::Track;

use super::bar_lookup::{BarLookup, ts_changes};
use super::state::SelectedItem;

mod auto;
mod note;
mod rows;

/// 事件浏览器显示所需的只读数据引用。
#[derive(Clone, Copy)]
pub struct EventBrowserData<'a> {
    /// 工程中的所有音轨。
    pub tracks: &'a [Track],
    /// 当前选中音轨的音符集合。
    pub current_track_notes: &'a im::Vector<Note>,
    /// 每四分音符 tick 数。
    pub ppq: u16,
    /// 拍号变化列表 `(tick, numerator, denominator)`。
    pub time_signatures: &'a [(u32, u8, u8)],
    /// 速度控制点。
    pub tempo_points: &'a [TempoPoint],
    /// 调号事件。
    pub key_signatures: &'a [KeySignatureEvent],
    /// 标记事件。
    pub markers: &'a [MarkerEvent],
    /// 歌词事件。
    pub lyrics: &'a [LyricsEvent],
    /// 和弦事件。
    pub chords: &'a [ChordEvent],
    /// 音色变换事件。
    pub program_changes: &'a [ProgramChangeEvent],
    /// 自动化事件 lane。
    pub automation_lanes: &'a [Arc<AutomationLane>],
}

/// 事件浏览器表格中的一行。
#[derive(Clone, Debug)]
pub(super) struct EventTableRow {
    /// 排序后的 0-based 行索引。
    pub id: usize,
    /// 事件 tick，用于选择、分页与跳转。
    pub tick: u32,
    /// 单元格文本（顺序与 `headers(item)` 一致）。
    pub cells: Vec<String>,
    /// 每个单元格对应的右键编辑请求（`#` 列为 `None`）。
    pub cell_edits: Vec<Option<EditRequest>>,
    /// 每个单元格对应的左键跳转请求（`#` 列为 `None`）。
    pub cell_jumps: Vec<Option<JumpRequest>>,
}

/// 按 `SelectedItem` 返回列标题与默认列宽。
pub(super) fn headers(item: &SelectedItem) -> &'static [(&'static str, f32)] {
    match item {
        SelectedItem::ProjectJson | SelectedItem::MappingJson => {
            &[("Key", 120.0), ("Value", 200.0)]
        }
        SelectedItem::Notes { .. } => &[
            ("#", 30.0),
            ("id", 50.0),
            ("tick", 55.0),
            ("position", 70.0),
            ("gate", 55.0),
            ("end_tick", 55.0),
            ("end_pos", 70.0),
            ("key", 45.0),
            ("velocity", 50.0),
            ("channel", 50.0),
        ],
        SelectedItem::Automation { .. } => &[
            ("#", 30.0),
            ("tick", 55.0),
            ("position", 70.0),
            ("value", 60.0),
            ("x1", 45.0),
            ("y1", 45.0),
            ("x2", 45.0),
            ("y2", 45.0),
            ("shape", 55.0),
        ],
        _ => &[
            ("#", 30.0),
            ("tick", 55.0),
            ("position", 70.0),
            ("value", 200.0),
        ],
    }
}

/// 按 `SelectedItem` 收集当前页应显示的所有行。
pub(super) fn collect_rows(item: &SelectedItem, data: &EventBrowserData<'_>) -> Vec<EventTableRow> {
    let bar_lookup = build_bar_lookup(data);
    let mut rows = match item {
        SelectedItem::TimeSig => rows::collect_time_sig_rows(data, &bar_lookup),
        SelectedItem::KeySig => rows::collect_key_sig_rows(data, &bar_lookup),
        SelectedItem::Markers => {
            rows::collect_text_rows(&bar_lookup, TextEventKind::Marker, data.markers, |m| {
                m.text.clone()
            })
        }
        SelectedItem::ConductorLyrics => rows::collect_text_rows(
            &bar_lookup,
            TextEventKind::ConductorLyrics,
            data.lyrics,
            |l| l.text.clone(),
        ),
        SelectedItem::ConductorChord => rows::collect_text_rows(
            &bar_lookup,
            TextEventKind::ConductorChord,
            data.chords,
            |c| c.text.clone(),
        ),
        SelectedItem::Notes { track } => note::collect_note_rows(data, &bar_lookup, *track),
        SelectedItem::ProgramChange { track } => rows::collect_pc_rows(data, &bar_lookup, *track),
        SelectedItem::Automation { track, target } => {
            auto::collect_auto_rows(data, &bar_lookup, *track, target)
        }
        SelectedItem::Lyrics { track } => rows::collect_text_rows(
            &bar_lookup,
            TextEventKind::Lyrics { track: *track },
            data.lyrics,
            |l| l.text.clone(),
        ),
        SelectedItem::Chord { track } => rows::collect_text_rows(
            &bar_lookup,
            TextEventKind::Chord { track: *track },
            data.chords,
            |c| c.text.clone(),
        ),
        SelectedItem::ProjectJson => rows::collect_project_rows(),
        SelectedItem::MappingJson => rows::collect_mapping_rows(),
    };

    rows.sort_by(|a, b| a.tick.cmp(&b.tick).then(a.id.cmp(&b.id)));
    for (id, row) in rows.iter_mut().enumerate() {
        row.id = id;
        if let Some(cell) = row.cells.first_mut() {
            *cell = id.to_string();
        }
    }
    rows
}

/// 构建小节位置转换器（默认 4/4，使用首个拍号分子）。
fn build_bar_lookup(data: &EventBrowserData<'_>) -> BarLookup {
    let default_num = data
        .time_signatures
        .first()
        .map(|(_, n, _)| *n)
        .unwrap_or(4);
    BarLookup::build(
        data.ppq as u32,
        default_num,
        &ts_changes(data.time_signatures),
    )
}

/// 构造跳转请求。
pub(super) fn make_jump(tick: u32, note: Option<(u16, u8)>) -> Option<JumpRequest> {
    Some(JumpRequest { tick, note })
}
