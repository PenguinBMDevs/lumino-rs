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

use crate::sidebar::core::Track;
use lumino_extras::i18n::MainTranslations;
use lumino_note_core::automation::AutomationLane;
use lumino_note_core::event::{
    ChordEvent, KeySignatureEvent, LyricsEvent, MarkerEvent, ProgramChangeEvent,
};
use lumino_note_core::midi_types::TempoPoint;
use lumino_ui_core::sidebar_event::{EditRequest, JumpRequest, TextEventKind};

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
    /// 当前选中音轨的音符集合（document 唯一权威，NoteEvent 切片）。
    pub current_track_notes: &'a [lumino_midi_loader::NoteEvent],
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
    // ── 工程元数据（用于 project/mapping 概览） ──
    /// 工程名称。
    pub project_name: &'a str,
    /// 工程作者。
    pub project_author: &'a str,
    /// 默认 BPM。
    pub project_bpm: f64,
    /// 精度（每四分音符 tick）。
    pub project_division: u16,
    /// 音轨总数。
    pub project_track_count: u16,
    /// 音符总数。
    pub project_note_count: u64,
    /// 创建时间。
    pub project_created: &'a str,
    /// 修改时间。
    pub project_modified: &'a str,
    /// 格式版本。
    pub project_format_version: u32,
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
pub(super) fn headers(item: &SelectedItem, t: &MainTranslations) -> Vec<(String, f32)> {
    match item {
        SelectedItem::ProjectJson | SelectedItem::MappingJson => {
            vec![
                (t.eb_key_colon.to_string(), 120.0),
                (t.eb_value_colon.to_string(), 200.0),
            ]
        }
        SelectedItem::Notes { .. } => vec![
            (t.eb_hash.to_string(), 30.0),
            (t.eb_id.to_string(), 50.0),
            (t.eb_tick.to_string(), 55.0),
            (t.eb_position.to_string(), 70.0),
            (t.eb_gate.to_string(), 55.0),
            (t.eb_end_tick.to_string(), 55.0),
            (t.eb_end_pos.to_string(), 70.0),
            (t.eb_key.to_string(), 45.0),
            (t.eb_velocity.to_string(), 50.0),
            (t.eb_channel.to_string(), 50.0),
        ],
        SelectedItem::Automation { .. } => vec![
            (t.eb_hash.to_string(), 30.0),
            (t.eb_tick.to_string(), 55.0),
            (t.eb_position.to_string(), 70.0),
            (t.eb_value.to_string(), 60.0),
            (t.eb_shape.to_string(), 55.0),
        ],
        _ => vec![
            (t.eb_hash.to_string(), 30.0),
            (t.eb_tick.to_string(), 55.0),
            (t.eb_position.to_string(), 70.0),
            (t.eb_value.to_string(), 200.0),
        ],
    }
}

/// 按 `SelectedItem` 收集当前页应显示的所有行。
pub(super) fn collect_rows(
    item: &SelectedItem,
    data: &EventBrowserData<'_>,
    t: &MainTranslations,
) -> Vec<EventTableRow> {
    let bar_lookup = build_bar_lookup(data);
    let mut rows = match item {
        SelectedItem::TimeSig => rows::collect_time_sig_rows(data, &bar_lookup, t),
        SelectedItem::KeySig => rows::collect_key_sig_rows(data, &bar_lookup, t),
        SelectedItem::Markers => rows::collect_text_rows(
            &bar_lookup,
            TextEventKind::Marker,
            data.markers,
            None,
            |m| m.text.clone(),
        ),
        SelectedItem::ConductorLyrics => rows::collect_text_rows(
            &bar_lookup,
            TextEventKind::ConductorLyrics,
            data.lyrics,
            Some(0),
            |l| l.text.clone(),
        ),
        SelectedItem::ConductorChord => rows::collect_text_rows(
            &bar_lookup,
            TextEventKind::ConductorChord,
            data.chords,
            Some(0),
            |c| c.text.clone(),
        ),
        SelectedItem::Notes { track } => note::collect_note_rows(data, &bar_lookup, *track),
        SelectedItem::ProgramChange { track } => rows::collect_pc_rows(data, &bar_lookup, *track),
        SelectedItem::Automation { track, target } => {
            auto::collect_auto_rows(data, &bar_lookup, *track, target, t)
        }
        SelectedItem::Lyrics { track } => rows::collect_text_rows(
            &bar_lookup,
            TextEventKind::Lyrics { track: *track },
            data.lyrics,
            Some(*track),
            |l| l.text.clone(),
        ),
        SelectedItem::Chord { track } => rows::collect_text_rows(
            &bar_lookup,
            TextEventKind::Chord { track: *track },
            data.chords,
            Some(*track),
            |c| c.text.clone(),
        ),
        SelectedItem::ProjectJson => rows::collect_project_rows(data, t),
        SelectedItem::MappingJson => rows::collect_mapping_rows(data, t),
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
