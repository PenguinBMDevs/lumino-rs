//! Runner 文件菜单：保存

use std::path::{Path, PathBuf};

use lumino_export::lmpj::extract_pc_cc_events;
use lumino_export::midi::{
    MidiExportData, MidiExportOptions, MidiNoteEvent, MidiTempoEvent, MidiTrackData,
};
use lumino_midi_loader::{ParsedMidi, bpm_to_tempo};

use crate::runner::RunnerInner;

use super::helpers::{get_file_extension, get_file_stem};

impl RunnerInner {
    /// 将编辑器内容保存为新的 MIDI 文件
    ///
    /// 用于从空白状态（未加载任何 MIDI/DMS 文件）保存用户编辑的音符。
    /// 设置 `current_midi_source` 并触发异步后台加载以填充 `current_midi`。
    pub(super) fn save_editor_as_midi_file(&mut self) -> Option<PathBuf> {
        let has_notes;
        let editor_notes;
        {
            let ui = self.window_state.window.ui();
            has_notes = ui.get_editor_note_count() > 0;
            if !has_notes {
                return None;
            }
            editor_notes = ui.get_editor_notes();
        }

        let save_path = rfd::FileDialog::new()
            .add_filter("MIDI 文件 (.mid)", &["mid"])
            .add_filter("MIDI 文件 (.midi)", &["midi"])
            .set_file_name("untitled.mid")
            .save_file()?;

        // 转换编辑器音符为 MIDI 导出数据（使用 1920 PPQN 保持黑乐谱精度）
        let tracks: Vec<MidiTrackData> = editor_notes
            .into_iter()
            .map(|(_, notes)| {
                let midi_notes: Vec<MidiNoteEvent> = notes
                    .into_iter()
                    .map(|(tick, key, length, velocity, channel)| MidiNoteEvent {
                        tick: (tick as u32).max(1),
                        channel,
                        key,
                        velocity,
                        duration: (length as u32).max(1),
                    })
                    .collect();
                MidiTrackData {
                    notes: midi_notes,
                    ..Default::default()
                }
            })
            .collect();

        let export_data = MidiExportData {
            options: MidiExportOptions {
                format: 1,
                ppqn: lumino_midi_loader::constants::DEFAULT_PPQN,
            },
            tracks,
        };

        // 写入 MIDI 文件
        if let Err(e) = lumino_export::export_midi(&save_path, &export_data) {
            tracing::error!("保存新项目失败: {}", e);
            return None;
        }

        // 设置源路径并触发后台加载（异步填充 current_midi）
        self.midi_state.current_midi_source = Some(save_path.clone());
        self.load_midi_file(save_path.clone());

        tracing::info!("新项目已保存为 MIDI 文件: {:?}", save_path);
        Some(save_path)
    }

    /// 保存文件（统一入口：显示格式选择对话框，支持 lmpj/mid/midi）
    pub(super) fn handle_save_file(&mut self) {
        self.handle_save_single_file();
    }

    /// 统一保存/导出为单文件：显示格式选择对话框，支持 lmpj/mid/midi
    pub(super) fn handle_save_single_file(&mut self) {
        let file_stem = self
            .midi_state
            .current_midi_source
            .as_ref()
            .or_else(|| self.midi_state.current_midi.as_ref().map(|m| &m.info.path))
            .map(|p| get_file_stem(Path::new(p)))
            .unwrap_or_else(|| "untitled".to_string());

        let Some(save_path) = rfd::FileDialog::new()
            .add_filter("Lumino MIDI Project", &["lmpj"])
            .add_filter("MIDI 文件 (.mid)", &["mid"])
            .add_filter("MIDI 文件 (.midi)", &["midi"])
            .set_file_name(format!("{file_stem}.lmpj"))
            .save_file()
        else {
            return;
        };

        let extension = get_file_extension(&save_path);

        match extension.as_str() {
            "lmpj" => self.save_as_lmpj_project(save_path),
            "mid" | "midi" => self.save_as_midi_with_edits(save_path),
            _ => tracing::warn!("不支持的保存格式: {}", extension),
        }
    }

    /// 保存为 LMPJ 文件（兼容旧版格式：zstd(bincode(LmpjData))，确保可重新加载）
    ///
    /// LMPJ 是本机工程格式，从内存中 `MidiDocument` 重建 MIDI 字节（含用户编辑的 tempo 等），
    /// **不依赖原始 .mid 文件**。保存时确保工程自包含——原始文件可删除后仍能完整加载。
    fn save_as_lmpj_project(&mut self, save_path: PathBuf) {
        let (info, midi_bytes) = if let Some(parsed_midi) = self.midi_state.current_midi.as_ref() {
            // 从 document 重建 MIDI 字节（含 tempo 编辑），始终返回 Some
            let bytes = self
                .maybe_rebuild_midi_with_tempo(parsed_midi)
                .expect("已加载的 MIDI 文件必有 document，重建不应失败");
            (parsed_midi.info.clone(), bytes)
        } else if let Some((ms, mb)) = self.export_editor_notes_as_legacy_lmpj() {
            (ms, mb)
        } else {
            tracing::warn!("没有加载的 MIDI 文件且没有编辑器内容，无法保存 LMPJ 格式");
            return;
        };

        let lmpj_data = lumino_midi_loader::LmpjData {
            info,
            midi_data: Some(midi_bytes),
        };

        let cb = self.window_state.progress_cb.clone();
        let save_path2 = save_path.clone();
        tokio::spawn(async move {
            cb("正在保存 LMPJ 文件", 0.3);
            match tokio::task::spawn_blocking(move || {
                let encoded = lumino_export::format::encode_lmpj(&lmpj_data)?;
                std::fs::write(&save_path2, encoded)
                    .map_err(|e| lumino_export::ExportError::Io(std::io::Error::other(e)))?;
                Ok::<(), lumino_export::ExportError>(())
            })
            .await
            {
                Ok(Ok(())) => {
                    cb("工程保存成功", 1.0);
                    tracing::info!("工程保存成功: {:?}", save_path);
                }
                Ok(Err(e)) => {
                    let msg = format!("保存失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
                Err(e) => {
                    let msg = format!("保存任务失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
            }
        });
    }

    /// 将编辑器音符导出为 MIDI 字节（内存中），返回 (MidiInfo, midi_bytes)
    fn export_editor_notes_as_legacy_lmpj(
        &mut self,
    ) -> Option<(lumino_midi_loader::MidiInfo, Vec<u8>)> {
        let has_notes = {
            let ui = self.window_state.window.ui();
            ui.get_editor_note_count() > 0
        };
        if !has_notes {
            return None;
        }

        // 提取源文档的 PC/CC 事件（如果有的话）
        let pc_cc_events = self
            .midi_state
            .current_midi
            .as_ref()
            .and_then(|pm| pm.document.as_ref())
            .map(|doc| extract_pc_cc_events(doc));

        let (notes, tempo_events) = {
            let ui = self.window_state.window.ui();
            let notes = ui.get_editor_notes();
            let tempos: Vec<MidiTempoEvent> = ui
                .root()
                .editor
                .editor_state
                .data
                .tempo_points
                .iter()
                .map(|tp| MidiTempoEvent {
                    tick: tp.tick as u32,
                    tempo: bpm_to_tempo(tp.bpm) as u32,
                })
                .collect();
            (notes, tempos)
        };

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
                let (program_changes, control_changes) = match &pc_cc_events {
                    Some((pc, cc)) => (
                        pc.get(&track_id).cloned().unwrap_or_default(),
                        cc.get(&track_id).cloned().unwrap_or_default(),
                    ),
                    None => (Vec::new(), Vec::new()),
                };
                MidiTrackData {
                    notes: midi_notes,
                    tempos: if i == 0 {
                        tempo_events.clone()
                    } else {
                        Vec::new()
                    },
                    program_changes,
                    control_changes,
                    ..Default::default()
                }
            })
            .collect();

        let export_data = MidiExportData {
            options: MidiExportOptions {
                format: 1,
                ppqn: lumino_midi_loader::constants::DEFAULT_PPQN,
            },
            tracks,
        };

        let midi_bytes = match lumino_export::midi::export_midi_to_bytes(&export_data) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("导出 MIDI 字节失败: {}", e);
                return None;
            }
        };

        let total_notes: u64 = notes.iter().map(|(_, n)| n.len() as u64).sum();
        let total_tracks = notes.len() as u16;
        let total_ticks = notes
            .iter()
            .flat_map(|(_, n)| n.iter())
            .map(|&(tick, _, len, _, _)| (tick + len) as u32)
            .max()
            .unwrap_or(0);

        let info = lumino_midi_loader::MidiInfo {
            path: Default::default(),
            track_count: total_tracks,
            total_notes,
            duration_ticks: total_ticks,
            division: 1920,
            parse_progress: Some(100.0),
        };

        Some((info, midi_bytes))
    }

    /// 当编辑器 tempo 与文档不一致时，重建 MIDI 字节（保留文档音符 + PC/CC 事件，替换 tempo 事件）
    fn maybe_rebuild_midi_with_tempo(&self, parsed_midi: &ParsedMidi) -> Option<Vec<u8>> {
        // 无条件重建：保证工程设置/指挥轨道 tempo 编辑总被保存
        let document = parsed_midi.document.as_ref()?;

        let (division, track_count) = (parsed_midi.info.division, parsed_midi.info.track_count);

        let tempo_events: Vec<MidiTempoEvent> = {
            let ui = self.window_state.window.ui();
            let root = ui.root();
            root.editor
                .editor_state
                .data
                .tempo_points
                .iter()
                .map(|tp| MidiTempoEvent {
                    tick: tp.tick as u32,
                    tempo: bpm_to_tempo(tp.bpm) as u32,
                })
                .collect()
        };

        // 提取 PC/CC 事件并按轨分组
        let (pc_by_track, cc_by_track) = extract_pc_cc_events(document);

        let mut tracks: Vec<MidiTrackData> = (0..track_count)
            .map(|track_id| {
                let doc_notes = document.get_track_notes(track_id);
                let midi_notes: Vec<MidiNoteEvent> = doc_notes
                    .iter()
                    .map(|&(tick, key, len, vel, ch)| MidiNoteEvent {
                        tick: (tick as u32).max(1),
                        channel: ch,
                        key,
                        velocity: vel,
                        duration: (len as u32).max(1),
                    })
                    .collect();
                MidiTrackData {
                    notes: midi_notes,
                    program_changes: pc_by_track.get(&track_id).cloned().unwrap_or_default(),
                    control_changes: cc_by_track.get(&track_id).cloned().unwrap_or_default(),
                    ..Default::default()
                }
            })
            .collect();

        if let Some(first) = tracks.first_mut() {
            first.tempos = tempo_events;
        }

        let export_data = MidiExportData {
            options: MidiExportOptions {
                format: 1,
                ppqn: division.max(1),
            },
            tracks,
        };

        lumino_export::midi::export_midi_to_bytes(&export_data).ok()
    }

    /// 保存为 MIDI（包含编辑器编辑 + 源文件的 PC/CC 事件）
    fn save_as_midi_with_edits(&mut self, save_path: PathBuf) {
        let editor_has_notes = {
            let ui = self.window_state.window.ui();
            ui.get_editor_note_count() > 0
        };

        if editor_has_notes {
            // 提取源文档的 PC/CC 事件（如果有的话）
            let pc_cc_events = self
                .midi_state
                .current_midi
                .as_ref()
                .and_then(|pm| pm.document.as_ref())
                .map(|doc| extract_pc_cc_events(doc));

            let (notes, tempo_events) = {
                let ui = self.window_state.window.ui();
                let notes = ui.get_editor_notes();
                let tempos: Vec<MidiTempoEvent> = ui
                    .root()
                    .editor
                    .editor_state
                    .data
                    .tempo_points
                    .iter()
                    .map(|tp| MidiTempoEvent {
                        tick: tp.tick as u32,
                        tempo: bpm_to_tempo(tp.bpm) as u32,
                    })
                    .collect();
                (notes, tempos)
            };

            let tracks: Vec<MidiTrackData> = notes
                .into_iter()
                .enumerate()
                .map(|(i, (_, notes))| {
                    let midi_notes: Vec<MidiNoteEvent> = notes
                        .into_iter()
                        .map(|(tick, key, length, velocity, channel)| MidiNoteEvent {
                            tick: (tick as u32).max(1),
                            channel,
                            key,
                            velocity,
                            duration: (length as u32).max(1),
                        })
                        .collect();
                    let track_id = i as u16;
                    let (program_changes, control_changes) = match &pc_cc_events {
                        Some((pc, cc)) => (
                            pc.get(&track_id).cloned().unwrap_or_default(),
                            cc.get(&track_id).cloned().unwrap_or_default(),
                        ),
                        None => (Vec::new(), Vec::new()),
                    };
                    MidiTrackData {
                        notes: midi_notes,
                        tempos: if i == 0 {
                            tempo_events.clone()
                        } else {
                            Vec::new()
                        },
                        program_changes,
                        control_changes,
                        ..Default::default()
                    }
                })
                .collect();

            let export_data = MidiExportData {
                options: MidiExportOptions {
                    format: 1,
                    ppqn: lumino_midi_loader::constants::DEFAULT_PPQN,
                },
                tracks,
            };

            let save_path2 = save_path.clone();
            tokio::spawn(async move {
                match tokio::task::spawn_blocking(move || {
                    lumino_export::export_midi(&save_path2, &export_data)
                })
                .await
                {
                    Ok(Ok(())) => tracing::info!("MIDI 保存成功: {:?}", save_path),
                    Ok(Err(e)) => tracing::error!("MIDI 保存失败: {}", e),
                    Err(e) => tracing::error!("MIDI 保存任务失败: {}", e),
                }
            });
            return;
        }

        // 无编辑器编辑，从已有源路径/文档导出
        let file_service = self.file_state.file_service.clone();
        if let Some(source_path) = &self.midi_state.current_midi_source {
            let source = source_path.clone();
            tokio::spawn(async move {
                let _ = file_service.save_as_midi(source, save_path).await;
            });
        } else if let Some(parsed_midi) = &self.midi_state.current_midi {
            let source = parsed_midi.info.path.clone();
            tokio::spawn(async move {
                let _ = file_service.save_as_midi(source, save_path).await;
            });
        }
    }
}
