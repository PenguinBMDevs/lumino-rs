//! Runner 文件菜单：编辑器内容 ↔ MIDI 数据构建工具
//!
//! 将 UI 编辑器中的音符、tempo 点与原始文档的 PC/CC 事件组合，
//! 生成可用于保存/导出的 `MidiDocument` 或 MIDI 字节。

use lumino_core::midi_types::TempoPoint;
use lumino_export::lmpj::extract_pc_cc_events;
use lumino_export::midi::{
    MidiExportData, MidiExportOptions, MidiNoteEvent, MidiTempoEvent, MidiTrackData,
};
use lumino_midi_loader::{MidiDocument, bpm_to_tempo, constants::DEFAULT_PPQN};
use std::collections::HashMap;

use crate::runner::RunnerInner;

/// 程序变更事件按轨分组
type ProgramChangeMap = HashMap<u16, Vec<lumino_export::midi::MidiProgramChangeEvent>>;
/// 控制变更事件按轨分组
type ControlChangeMap = HashMap<u16, Vec<lumino_export::midi::MidiControlChangeEvent>>;

/// 从当前加载的文档或编辑器构建一个临时 `MidiDocument`
///
/// 优先使用已加载的 `MidiDocument`；若已释放，则尝试从编辑器音符重建。
/// 返回 `None` 表示既没有已加载文档也没有编辑器内容。
pub(super) fn build_editor_midi_document(runner: &RunnerInner) -> Option<MidiDocument> {
    if let Some(doc) = runner
        .midi_state
        .current_midi
        .as_ref()
        .and_then(|pm| pm.document.as_deref().cloned())
    {
        return Some(doc);
    }

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

/// 从当前已加载文档提取 PC/CC 事件
fn extract_current_pc_cc(runner: &RunnerInner) -> Option<(ProgramChangeMap, ControlChangeMap)> {
    runner
        .midi_state
        .current_midi
        .as_ref()
        .and_then(|pm| pm.document.as_ref())
        .map(|doc| extract_pc_cc_events(std::sync::Arc::as_ref(doc)))
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
