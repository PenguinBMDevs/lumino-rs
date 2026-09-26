//! Runner 文件菜单：编辑器内容 ↔ MIDI 数据构建工具
//!
//! 将 UI 编辑器中的音符、tempo 点与原始文档的 PC/CC 事件组合，
//! 生成可用于保存/导出的 `MidiDocument` 或 MIDI 字节。

use lumino_export::midi::{
    DocPassthrough, MidiExportData, MidiExportOptions, MidiNoteEvent, MidiTempoEvent,
    MidiTimeSignatureEvent, MidiTrackData, ReleaseMap, build_release_map,
    extract_passthrough_events, extract_pc_cc_events,
};
use lumino_midi_loader::{MidiDocument, bpm_to_tempo};
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
    current_document(runner).map(extract_pc_cc_events)
}

/// 从当前已加载文档提取透传事件 + 释放力度匹配表（经 UI 只读借用）
fn extract_current_passthrough(runner: &RunnerInner) -> Option<(DocPassthrough, ReleaseMap)> {
    current_document(runner).map(|doc| (extract_passthrough_events(doc), build_release_map(doc)))
}

/// 当前已加载文档（UI 编辑器数据的单一权威源，只读借用）
fn current_document(runner: &RunnerInner) -> Option<&MidiDocument> {
    runner
        .window_state
        .window
        .ui()
        .root()
        .editor
        .editor_state
        .data
        .document
        .as_ref()
}

/// 当前已加载文档的轨道名（索引 = 轨号，文档缺失时返回空）
fn current_track_names(runner: &RunnerInner) -> Vec<Option<String>> {
    current_document(runner).map_or_else(Vec::new, |doc| doc.track_names.to_vec())
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
    // 使用编辑器当前 PPQ（用户可在工具栏修改），不硬编码默认值——
    // 否则新工程保存的 MIDI/工程文件 PPQ 与 UI 显示不一致（同类 BUG 一并修复）。
    let ppq = runner.window_state.window.ui().ppq();
    let time_signatures = editor_time_signatures(runner);
    let pc_cc = extract_current_pc_cc(runner);
    // 透传事件 + 释放力度匹配表：编辑器未建模的内容（弯音/触后/文本/SysEx/端口/
    // 调号）与未编辑音符的释放力度全部从已加载文档回查，保存=无损往返。
    let (pass, mut release_map): (DocPassthrough, ReleaseMap) =
        extract_current_passthrough(runner).unwrap_or_default();
    let track_names = current_track_names(runner);

    let tracks: Vec<MidiTrackData> = notes
        .iter()
        .enumerate()
        .map(|(i, (_, notes))| {
            let track_id = i as u16;
            let midi_notes: Vec<MidiNoteEvent> = notes
                .iter()
                .map(|&(tick, key, length, velocity, channel)| {
                    let tick_u32 = tick as u32;
                    // tick=0 与零长度音符按原样写出，不再钳制为 1
                    // （钳制会挪动音符位置/拉长零长度音符，破坏往返等价）。
                    let release_velocity = release_map
                        .get_mut(&track_id)
                        .and_then(|by_key| by_key.get_mut(&(tick_u32, key, channel))?.pop_front())
                        .unwrap_or(0);
                    MidiNoteEvent {
                        tick: tick_u32,
                        channel,
                        key,
                        velocity,
                        release_velocity,
                        duration: length as u32,
                    }
                })
                .collect();
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
                // 调号编辑器未建模：文档有则透传到首轨（与 tempo/拍号同位）
                key_signatures: if tempos_on_first_track && i == 0 {
                    pass.key_signatures.clone()
                } else {
                    Vec::new()
                },
                program_changes,
                control_changes,
                pitch_bends: pass.pitch_bends.get(&track_id).cloned().unwrap_or_default(),
                channel_aftertouch: pass
                    .channel_aftertouch
                    .get(&track_id)
                    .cloned()
                    .unwrap_or_default(),
                poly_aftertouch: pass
                    .poly_aftertouch
                    .get(&track_id)
                    .cloned()
                    .unwrap_or_default(),
                lyrics: pass.lyrics.get(&track_id).cloned().unwrap_or_default(),
                markers: pass.markers.get(&track_id).cloned().unwrap_or_default(),
                text_events: pass.text_events.get(&track_id).cloned().unwrap_or_default(),
                sys_ex: pass.sys_ex.get(&track_id).cloned().unwrap_or_default(),
                midi_port: pass.midi_ports.get(&track_id).copied(),
                name: track_names.get(i).cloned().flatten(),
            }
        })
        .collect();

    Some(MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: ppq,
        },
        tracks,
    })
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
    // 注意：不要合并为单个混合类型（f32 tick + f64 bpm）的 map 循环——
    // SLP 向量化器对该模式生成非法 shufflevector（f32 与 f64 操作数互插），
    // 在 AArch64/ARMv7 后端触发 "Do not know how to split this operator's operand!"
    // 崩溃（LLVM 22.1，rustc 1.97；x86_64 因 AVX 恰好不炸）。拆成两个同质循环后
    // zip，逐元素运算完全等价。
    let ticks: Vec<u32> = points.iter().map(|tp| tp.tick as u32).collect();
    let tempos: Vec<u32> = points.iter().map(|tp| bpm_to_tempo(tp.bpm)).collect();
    ticks
        .into_iter()
        .zip(tempos)
        .map(|(tick, tempo)| MidiTempoEvent { tick, tempo })
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
