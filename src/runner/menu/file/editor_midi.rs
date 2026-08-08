//! Runner 文件菜单：编辑器内容 ↔ MIDI 数据构建工具
//!
//! 将 UI 编辑器中的音符、tempo 点与原始文档的 PC/CC 事件组合，
//! 生成可用于保存/导出的 `MidiDocument` 或 MIDI 字节。

use lumino_export::midi::{
    MidiExportData, MidiExportOptions, MidiNoteEvent, MidiTempoEvent, MidiTimeSignatureEvent,
    MidiTrackData, extract_pc_cc_events,
};
use lumino_midi_loader::{MidiDocument, bpm_to_tempo, constants::DEFAULT_PPQN};
use lumino_note_core::midi_types::TempoPoint;
use std::collections::HashMap;

use crate::runner::RunnerInner;

/// 程序变更事件按轨分组
type ProgramChangeMap = HashMap<u16, Vec<lumino_export::midi::MidiProgramChangeEvent>>;
/// 控制变更事件按轨分组
type ControlChangeMap = HashMap<u16, Vec<lumino_export::midi::MidiControlChangeEvent>>;

/// 从当前加载的文档或编辑器构建一个临时 `MidiDocument`
///
/// 优先使用 UI 中已加载的 `MidiDocument`（单一权威源，零拷贝借用）；
/// 若已释放，则尝试从编辑器音符重建。
/// 返回 `None` 表示既没有已加载文档也没有编辑器内容。
pub(super) fn build_editor_midi_document(runner: &RunnerInner) -> Option<MidiDocument> {
    // 2026-08 单一权威源改造：不再从 runner.midi_state 深拷贝 document，
    // 优先借用 UI 的 EditorData.document；调用方需要所有权时（如 LMPJ 保存）
    // 由 save.rs 直接借用，本函数仅作为「无文档时从编辑器重建」的回退路径。
    let export_data = build_midi_export_data_from_editor(runner, true)?;
    let midi_bytes = export_midi_to_bytes(&export_data)?;
    match MidiDocument::from_notes_bytes(&midi_bytes, None) {
        Ok((doc, _, _)) => Some(doc),
        Err(e) => {
            tracing::error!("从编辑器 MIDI 字节构建 MidiDocument 失败: {}", e);
            None
        }
    }
}

/// 编辑器音符表示
pub(super) type EditorNotes = Vec<(usize, Vec<(f32, u8, f32, u8, u8)>)>;

/// 从当前已加载文档提取 PC/CC 事件（经 UI 只读借用，零拷贝）
fn extract_current_pc_cc(runner: &RunnerInner) -> Option<(ProgramChangeMap, ControlChangeMap)> {
    let ui = runner.window_state.window.ui();
    ui.root()
        .editor
        .editor_state
        .data
        .document
        .as_ref()
        .map(extract_pc_cc_events)
}

/// 读取编辑器中的音符与 tempo 点
fn editor_notes_and_tempos(runner: &RunnerInner) -> Option<(EditorNotes, Vec<TempoPoint>)> {
    let ui = runner.window_state.window.ui();
    let notes = ui.get_editor_notes();
    if notes.iter().all(|(_, n)| n.is_empty()) {
        return None;
    }
    let tempos = ui.root().editor.editor_state.data.tempo_points.clone();
    Some((notes, tempos))
}

/// 根据编辑器内容构造 `MidiExportData`
///
/// `tempos_on_first_track` 控制 tempo 事件是否只放在第一轨。
pub(super) fn build_midi_export_data_from_editor(
    runner: &RunnerInner,
    tempos_on_first_track: bool,
) -> Option<MidiExportData> {
    let (notes, tempo_points) = editor_notes_and_tempos(runner)?;
    let time_signatures = editor_time_signatures(runner);
    let pc_cc = extract_current_pc_cc(runner);

    let tracks: Vec<MidiTrackData> = notes
        .iter()
        .enumerate()
        .map(|(i, (_, notes))| {
            let midi_notes: Vec<MidiNoteEvent> = notes
                .iter()
                .map(|&(tick, key, length, velocity, channel)| MidiNoteEvent {
                    tick: (tick as u32).max(1),
                    channel,
                    key,
                    velocity,
                    duration: (length as u32).max(1),
                })
                .collect();
            let track_id = i as u16;
            let (program_changes, control_changes) = match &pc_cc {
                Some((pc, cc)) => (
                    pc.get(&track_id).cloned().unwrap_or_default(),
                    cc.get(&track_id).cloned().unwrap_or_default(),
                ),
                None => (Vec::new(), Vec::new()),
            };
            MidiTrackData {
                notes: midi_notes,
                tempos: if tempos_on_first_track && i == 0 {
                    tempo_events_from_points(&tempo_points)
                } else {
                    Vec::new()
                },
                time_signatures: if tempos_on_first_track && i == 0 {
                    time_signatures.clone()
                } else {
                    Vec::new()
                },
                program_changes,
                control_changes,
                ..Default::default()
            }
        })
        .collect();

    Some(MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: DEFAULT_PPQN,
        },
        tracks,
    })
}

/// 从选区音符**直接**构建素材工程（.lmmaterial 专用，不走 MIDI 字节中转）
///
/// 设计动机（修复音符长度错乱）：
/// - MIDI 中转（字节编码 → 回读）存在 f32→u32 截断与 midly NoteOn/NoteOff
///   配对语义差异，同 key 相接/重叠音符回读后长度错乱（部分变短部分变长）；
/// - 本函数直接以整数 tick 构建 `CompactEvent`（精确 round），配合稳定排序
///   （同 tick NoteOff 在 NoteOn 前）与回读端 FIFO 配对，长度无损。
///
/// - 音符：仅选中音符（跨轨）；
/// - 控制事件（CC/PC/弯音）：仅选中轨道；
/// - 全局数据（tempo/拍号/调号/歌词/SysEx）：全量保留。
pub(super) fn build_material_project_from_selection(
    doc: &MidiDocument,
    selected: &EditorNotes,
) -> lumino_export::LuminoProject {
    use lumino_midi_loader::{CompactEvent, EventKind};
    use lumino_project::{LmtrackData, TrackMeta, TrackVisibilitySer};

    let mut project = lumino_export::LuminoProject::new("Material");
    project.metadata.audio.division = doc.division;
    project.metadata.audio.total_ticks = doc.total_ticks;
    project.tempo_changes = doc.tempo_changes.clone();
    project.time_signatures = doc.time_signatures.clone();
    project.key_signatures = doc.key_signatures.clone();
    project.lyrics = doc.lyrics.clone();
    project.markers = doc.markers.clone();
    project.sys_ex = doc.sys_ex.clone();
    project.track_names = doc.track_names.clone();

    // 选中轨道集合（控制事件过滤基准）
    let selected_tracks: std::collections::HashSet<u16> =
        selected.iter().map(|(t, _)| *t as u16).collect();

    // 控制事件：仅保留选中轨道的自动化数据
    for ev in &doc.control_events {
        let ev_track = ev.track; // packed 字段先拷贝，避免未对齐引用
        if !selected_tracks.contains(&ev_track) {
            continue;
        }
        match ev.kind {
            0 => {
                let (controller, value) = ev.as_control_change();
                project
                    .control_changes
                    .push((ev.tick, ev.track, ev.channel, controller, value));
            }
            1 => {
                let program = ev.as_program_change();
                project
                    .program_changes
                    .push((ev.tick, ev.track, ev.channel, program));
            }
            2 => {
                let normalized = ev.as_pitch_bend();
                let offset = (normalized * 8192.0).round() as i16;
                project
                    .pitch_bends
                    .push((ev.tick, ev.track, ev.channel, offset));
            }
            _ => {}
        }
    }

    // 每轨构建 CompactEvent（精确 round，无 f32→u32 截断）
    let mut total_notes: u64 = 0;
    for (track_id, notes) in selected {
        let mut track_events: Vec<CompactEvent> = Vec::with_capacity(notes.len() * 2);
        for &(tick, key, length, velocity, channel) in notes {
            let start = tick.round();
            let end = (tick + length).round().max(start + 1.0);
            track_events.push(CompactEvent::new(
                start as u32,
                *track_id as u16,
                EventKind::NoteOn,
                channel,
                key as u16,
                velocity as u16,
            ));
            track_events.push(CompactEvent::new(
                end as u32,
                *track_id as u16,
                EventKind::NoteOff,
                channel,
                key as u16,
                velocity as u16,
            ));
        }
        // 稳定排序（保持声明顺序：同 tick 的 NoteOff 先于后续音符的 NoteOn）
        track_events.sort_by_key(|e| e.delta_tick());

        // 绝对 tick → 相对 delta_tick
        let mut last_tick = 0_u32;
        for ev in &mut track_events {
            let abs_tick = ev.delta_tick();
            ev.set_delta_tick(abs_tick.saturating_sub(last_tick));
            last_tick = abs_tick;
        }

        let channel = track_events
            .iter()
            .find(|ev| ev.kind().is_note())
            .map(|ev| ev.channel())
            .unwrap_or(0);
        let max_tick = track_events
            .iter()
            .scan(0_u32, |acc, ev| {
                *acc = acc.saturating_add(ev.delta_tick());
                Some(*acc)
            })
            .last()
            .unwrap_or(0);
        let name = doc.track_name(*track_id).unwrap_or("").to_string();

        let meta = TrackMeta {
            track_id: *track_id as u16,
            name,
            channel,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: channel == 9,
            max_tick,
        };
        let track_data = LmtrackData::from_compact_events(meta, &track_events);
        project.add_track(track_data);
        total_notes += notes.len() as u64;
    }

    project.metadata.audio.total_notes = total_notes;
    project.metadata.audio.track_count = selected.len() as u16;
    project
}

/// 读取编辑器中的拍号变化列表
fn editor_time_signatures(runner: &RunnerInner) -> Vec<MidiTimeSignatureEvent> {
    let ui = runner.window_state.window.ui();
    ui.root()
        .editor
        .editor_state
        .data
        .time_signatures
        .iter()
        .map(|(tick, numerator, denominator)| MidiTimeSignatureEvent {
            tick: *tick,
            numerator: *numerator,
            denominator: human_denominator_to_power_of_two(*denominator),
            clocks_per_tick: 24,
            notated_32nd_notes_per_beat: 8,
        })
        .collect()
}

/// 将人类可读分母（4/8/16）转换为 MIDI 标准 2 的幂次
fn human_denominator_to_power_of_two(denominator: u8) -> u8 {
    match denominator {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        16 => 4,
        32 => 5,
        64 => 6,
        _ => {
            tracing::warn!("不常见的拍号分母: {}，回退到 4", denominator);
            2
        }
    }
}

/// 将内部 tempo 点类型转换为导出用 `MidiTempoEvent`
fn tempo_events_from_points(points: &[TempoPoint]) -> Vec<MidiTempoEvent> {
    points
        .iter()
        .map(|tp| MidiTempoEvent {
            tick: tp.tick as u32,
            tempo: bpm_to_tempo(tp.bpm) as u32,
        })
        .collect()
}

/// 将 `MidiExportData` 导出为 MIDI 字节
fn export_midi_to_bytes(export_data: &MidiExportData) -> Option<Vec<u8>> {
    match lumino_export::midi::export_midi_to_bytes(export_data) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            tracing::error!("导出 MIDI 字节失败: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_human_denominator_to_power_of_two() {
        assert_eq!(human_denominator_to_power_of_two(1), 0);
        assert_eq!(human_denominator_to_power_of_two(2), 1);
        assert_eq!(human_denominator_to_power_of_two(4), 2);
        assert_eq!(human_denominator_to_power_of_two(8), 3);
        assert_eq!(human_denominator_to_power_of_two(16), 4);
        assert_eq!(human_denominator_to_power_of_two(32), 5);
        assert_eq!(human_denominator_to_power_of_two(64), 6);
    }

    #[test]
    fn test_human_denominator_to_power_of_two_fallback() {
        assert_eq!(human_denominator_to_power_of_two(3), 2);
        assert_eq!(human_denominator_to_power_of_two(128), 2);
    }
}
