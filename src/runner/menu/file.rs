//! Runner 文件菜单处理

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumino_midi_loader::{ParsedMidi, bpm_to_tempo};
use lumino_ui::event;
use lumino_export::midi::{
    MidiExportData, MidiExportOptions, MidiNoteEvent, MidiTempoEvent, MidiTrackData,
};

use crate::runner::{RunnerInner, async_helper::run_async_task};

/// 获取文件扩展名（小写）
fn get_file_extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

/// 从文件路径获取文件名（不含扩展名），失败时返回 "untitled"
fn get_file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "untitled".to_string())
}

impl RunnerInner {
    /// 处理文件菜单事件
    pub(super) fn handle_file_menu_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        file_event: lumino_ui::event::menu::file::Event,
    ) {
        use lumino_ui::event::menu::file::Event::*;

        match file_event {
            Exit => event_loop.exit(),
            New => self.handle_new_file(),
            Open => self.handle_open_file(),
            ImportFiles => self.handle_import_files(),
            Save => self.handle_save_file(),
            MidiLoaded(info) => {
                tracing::info!("MIDI 文件加载完成：{}", info);
            }
            MidiLoadError(err) => {
                tracing::error!("MIDI 文件加载失败：{}", err);
            }
            MidiParsed(parsed) => {
                tracing::info!("MIDI 文件解析完成：{}", parsed.info);

                // 先导入音符到编辑器（新的懒加载模式：只加载当前音轨，其他音轨按需加载）
                self.import_midi_to_editor(&parsed);

                tracing::debug!("MIDI 文档已导入编辑器，MidiDocument 保留供懒加载使用");

                self.log_memory_usage_after_import();

                // 保留 current_midi 使 MidiDocument 存活（编辑器通过 Arc 引用它做懒加载）
                // 不再需要保存一份全量 track_notes，所以总内存从 (events+notes) 降到 (events)
                self.midi_state.current_midi_source =
                    Some(std::path::PathBuf::from(&parsed.info.path));
                self.midi_state.current_midi = Some(parsed);

                if let Some(state) = &mut self.test_state.test_mode_state {
                    state.active = true;
                }
            }
            MidiParseError(err) => {
                tracing::error!("MIDI 文件解析失败：{}", err);
                if self.test_state.test_mode_state.is_some() {
                    tracing::error!("测试模式因 MIDI 加载失败而退出");
                    event_loop.exit();
                }
            }
            DmsParsed(parsed) => {
                tracing::info!("DMS 文件解析完成：{}", parsed.info);

                // 导入 DMS 到编辑器
                self.import_dms_to_editor(&parsed);

                self.midi_state.current_dms = Some(parsed);
            }
            DmsParseError(err) => {
                tracing::error!("DMS 文件解析失败：{}", err);
            }
            Close => {
                self.midi_state.current_midi = None;
                self.midi_state.current_midi_source = None;
                self.midi_state.current_dms = None;
                self.window_state.window.ui_mut().clear_editor();
                tracing::info!("工程已关闭");
            }
            ProjectSettings => {
                let saved_title = self.window_state.window.ui().get_project_settings_title();
                let display_title = if saved_title.is_empty() {
                    self.midi_state
                        .current_midi_source
                        .as_ref()
                        .and_then(|p| p.file_stem())
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "无标题".to_string())
                } else {
                    saved_title
                };
                let title = format!("{} - Lumino Midi", display_title);
                self.window_state
                    .dialog_manager
                    .open_project_settings(title);
            }
            Settings => {
                self.window_state
                    .dialog_manager
                    .open_dialog(crate::runner::dialog_manager::DialogType::Settings);
            }
            TrackSelected(track_idx) => {
                // 统一使用 cache-only 模式，只切换音轨索引
                // 播放时从 cache 流式读取，不单独加载音轨到编辑器
                tracing::info!("切换到音轨：{}", track_idx);
                self.window_state
                    .window
                    .ui_mut()
                    .set_current_track(track_idx);
            }
            ExportProjectArchive => {
                self.handle_export_project_archive();
            }
            ExportProjectFolder => {
                self.handle_export_project_folder();
            }
            AudioExport => {
                self.handle_audio_export();
            }
            _ => {
                tracing::debug!("未处理的文件事件：{:?}", file_event);
            }
        }
    }

    /// 创建新文件
    pub(super) fn handle_new_file(&mut self) {
        // 清空当前工程
        self.midi_state.current_midi = None;
        self.midi_state.current_midi_source = None;
        self.midi_state.current_dms = None;

        // 清空编辑器
        self.window_state.window.ui_mut().clear_editor();

        tracing::info!("已创建新工程");
    }

    /// 打开文件
    pub(super) fn handle_open_file(&mut self) {
        // 使用 FileHandler 打开文件对话框
        let Some(path) = self.file_state.file_handler.handle_open_file() else {
            return;
        };

        let extension = get_file_extension(&path);

        if extension == "dms" {
            self.load_dms_file(path);
            return;
        }

        // 检查文件大小，大文件弹出确认对话框
        const MEMORY_OPTIMIZE_THRESHOLD_MB: u64 = 100;
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let size_mb = file_size as f64 / (1024.0 * 1024.0);

        if size_mb > MEMORY_OPTIMIZE_THRESHOLD_MB as f64 {
            let path_str = path.to_string_lossy().to_string();
            // 存储 pending 路径，等待对话框结果
            self.file_state.pending_load_path = Some(path);
            // 使用已有 dialog 窗口基础设施
            self.window_state
                .dialog_manager
                .open_load_confirm(path_str, size_mb);
        } else {
            tracing::info!("文件大小 {:.1}MB ≤ 阈值，标准模式加载", size_mb);
            self.load_midi_file(path);
        }
    }

    /// 加载 DMS 文件
    pub(super) fn load_dms_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 DMS 文件：{:?}", path);
        let progress_cb = self.window_state.progress_cb.clone();
        tokio::spawn(async move {
            run_async_task(
                lumino_midi_loader::loader::load_dms(path, Some(&progress_cb)),
                |parsed| event!(Menu.File.DmsParsed(Arc::new(parsed))),
                |e| event!(Menu.File.DmsParseError(e)),
            )
            .await;
        });
    }

    /// 加载 MIDI 文件
    pub(crate) fn load_midi_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 MIDI 文件：{:?}", path);
        let progress_cb = self.window_state.progress_cb.clone();
        tokio::spawn(async move {
            run_async_task(
                lumino_midi_loader::loader::load_parsed_midi(path, Some(&progress_cb)),
                |parsed| event!(Menu.File.MidiParsed(std::sync::Arc::new(parsed))),
                |e| event!(Menu.File.MidiParseError(e)),
            )
            .await;
        });
    }

    /// 导入文件
    pub(super) fn handle_import_files(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                crate::constants::filters::MUSIC_FILES.0,
                crate::constants::filters::MUSIC_FILES.1,
            )
            .add_filter(
                crate::constants::filters::MIDI_FILES.0,
                crate::constants::filters::MIDI_FILES.1,
            )
            .add_filter(
                crate::constants::filters::LUMINO_PROJECT.0,
                crate::constants::filters::LUMINO_PROJECT.1,
            )
            .add_filter(
                crate::constants::filters::DOMINO_PROJECT.0,
                crate::constants::filters::DOMINO_PROJECT.1,
            )
            .add_filter(
                crate::constants::filters::ALL_FILES.0,
                crate::constants::filters::ALL_FILES.1,
            )
            .pick_file()
        else {
            return;
        };

        let extension = get_file_extension(&path);

        if extension == "dms" {
            tracing::info!("开始后台导入 DMS 文件：{:?}", path);
            let progress_cb = self.window_state.progress_cb.clone();
            tokio::spawn(async move {
                run_async_task(
                    lumino_midi_loader::loader::load_dms(path, Some(&progress_cb)),
                    |parsed| event!(Menu.File.DmsParsed(Arc::new(parsed))),
                    |e| event!(Menu.File.DmsParseError(e)),
                )
                .await;
            });
            return;
        }

        // 检查文件大小，大文件弹出确认对话框
        const MEMORY_OPTIMIZE_THRESHOLD_MB: u64 = 100;
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let size_mb = file_size as f64 / (1024.0 * 1024.0);

        if size_mb > MEMORY_OPTIMIZE_THRESHOLD_MB as f64 {
            let path_str = path.to_string_lossy().to_string();
            self.file_state.pending_load_path = Some(path);
            self.window_state
                .dialog_manager
                .open_load_confirm(path_str, size_mb);
        } else {
            tracing::info!("导入文件 {:.1}MB ≤ 阈值，标准模式加载", size_mb);
            let progress_cb = self.window_state.progress_cb.clone();
            tokio::spawn(async move {
                run_async_task(
                    lumino_midi_loader::loader::load_parsed_midi(path, Some(&progress_cb)),
                    |parsed| event!(Menu.File.MidiParsed(std::sync::Arc::new(parsed))),
                    |e| event!(Menu.File.MidiParseError(e)),
                )
                .await;
            });
        }
    }

    /// 将编辑器内容保存为新的 MIDI 文件
    ///
    /// 用于从空白状态（未加载任何 MIDI/DMS 文件）保存用户编辑的音符。
    /// 设置 `current_midi_source` 并触发异步后台加载以填充 `current_midi`。
    fn save_editor_as_midi_file(&mut self) -> Option<PathBuf> {
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
                ppqn: 1920,
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

    /// 保存文件（统一入口：显示格式选择对话框，支持 lmpj/mid/midi/dms）
    pub(super) fn handle_save_file(&mut self) {
        self.handle_save_single_file();
    }

    /// 统一保存/导出为单文件：显示格式选择对话框，支持 lmpj/mid/midi/dms
    fn handle_save_single_file(&mut self) {
        let file_stem = self
            .midi_state
            .current_midi_source
            .as_ref()
            .or_else(|| self.midi_state.current_midi.as_ref().map(|m| &m.info.path))
            .or_else(|| self.midi_state.current_dms.as_ref().map(|d| &d.info.path))
            .map(|p| get_file_stem(Path::new(p)))
            .unwrap_or_else(|| "untitled".to_string());

        let Some(save_path) = rfd::FileDialog::new()
            .add_filter("Lumino MIDI Project", &["lmpj"])
            .add_filter("MIDI 文件 (.mid)", &["mid"])
            .add_filter("MIDI 文件 (.midi)", &["midi"])
            .add_filter("Domino 项目", &["dms"])
            .set_file_name(format!("{file_stem}.lmpj"))
            .save_file()
        else {
            return;
        };

        let extension = get_file_extension(&save_path);

        match extension.as_str() {
            "lmpj" => self.save_as_lmpj_project(save_path),
            "mid" | "midi" => self.save_as_midi_with_edits(save_path),
            "dms" => self.save_as_dms_from_source(save_path),
            _ => tracing::warn!("不支持的保存格式: {}", extension),
        }
    }

    /// 保存为 LMPJ 文件（兼容旧版格式：zstd(bincode(LmpjData))，确保可重新加载）
    fn save_as_lmpj_project(&mut self, save_path: PathBuf) {
        let (info, midi_bytes) = if let Some(parsed_midi) = self.midi_state.current_midi.as_ref() {
            let bytes = match self.maybe_rebuild_midi_with_tempo(parsed_midi) {
                Some(b) => b,
                None => match parsed_midi.get_midi_bytes() {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!("获取 MIDI 数据失败: {}", e);
                        return;
                    }
                },
            };
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
    fn export_editor_notes_as_legacy_lmpj(&mut self) -> Option<(lumino_midi_loader::MidiInfo, Vec<u8>)> {
        let has_notes = {
            let ui = self.window_state.window.ui();
            ui.get_editor_note_count() > 0
        };
        if !has_notes {
            return None;
        }

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
                MidiTrackData {
                    notes: midi_notes,
                    tempos: if i == 0 {
                        tempo_events.clone()
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                }
            })
            .collect();

        let export_data = MidiExportData {
            options: MidiExportOptions {
                format: 1,
                ppqn: 1920,
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

    /// 当编辑器 tempo 与文档不一致时，重建 MIDI 字节（保留文档音符，替换 tempo 事件）
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

    /// 保存为 MIDI（包含编辑器编辑）
    fn save_as_midi_with_edits(&mut self, save_path: PathBuf) {
        let editor_has_notes = {
            let ui = self.window_state.window.ui();
            ui.get_editor_note_count() > 0
        };

        if editor_has_notes {
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
                    MidiTrackData {
                        notes: midi_notes,
                        tempos: if i == 0 {
                            tempo_events.clone()
                        } else {
                            Vec::new()
                        },
                        ..Default::default()
                    }
                })
                .collect();

            let export_data = MidiExportData {
                options: MidiExportOptions {
                    format: 1,
                    ppqn: 1920,
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

    /// 从 DMS 源保存
    fn save_as_dms_from_source(&self, save_path: PathBuf) {
        let Some(parsed_dms) = self.midi_state.current_dms.as_ref() else {
            tracing::warn!("没有加载 DMS 文件，无法保存为 DMS 格式");
            return;
        };
        let source = parsed_dms.info.path.clone();
        let file_service = self.file_state.file_service.clone();
        tokio::spawn(async move {
            let _ = file_service.export_dms_to_midi(source, save_path).await;
        });
    }

    /// 导出为单文件（统一入口：与"保存"共享相同格式选择对话框）
    pub(super) fn handle_export_project_archive(&mut self) {
        self.handle_save_single_file();
    }

    /// 导出工程为文件夹
    pub(super) fn handle_export_project_folder(&mut self) {
        // 如果没有加载 MIDI 但有编辑器内容，先自动保存
        if self.midi_state.current_midi.is_none() && self.midi_state.current_midi_source.is_none() {
            let has_notes = {
                let ui = self.window_state.window.ui();
                ui.get_editor_note_count() > 0
            };
            if has_notes {
                tracing::info!("导出工程：自动保存新项目");
                if self.save_editor_as_midi_file().is_none() {
                    return; // 用户取消保存
                }
                // 阻塞加载刚保存的 MIDI 文件以获取完整文档
                if let Some(ref source) = self.midi_state.current_midi_source.clone() {
                    match futures::executor::block_on(lumino_midi_loader::loader::load_midi(
                        source.clone(),
                    )) {
                        Ok(parsed) => {
                            self.midi_state.current_midi = Some(Arc::new(parsed));
                        }
                        Err(e) => {
                            tracing::error!("自动保存后加载 MIDI 失败: {}", e);
                            return;
                        }
                    }
                } else {
                    return;
                }
            }
        }

        let Some(parsed_midi) = self.midi_state.current_midi.as_ref() else {
            tracing::warn!("没有加载的 MIDI 文件，无法导出工程");
            return;
        };

        let Some(document) = parsed_midi.document.as_ref() else {
            tracing::warn!("MidiDocument 已释放，无法导出工程");
            return;
        };

        let file_stem = get_file_stem(Path::new(&parsed_midi.info.path));

        let Some(save_path) = rfd::FileDialog::new()
            .set_file_name(format!("{file_stem}.lmpj"))
            .pick_folder()
        else {
            return;
        };

        let project = lumino_export::LuminoProject::from_midi_document(document);
        let cb = self.window_state.progress_cb.clone();

        tokio::spawn(async move {
            cb("准备导出工程", 0.0);
            cb("正在导出工程", 0.3);

            let path_clone = save_path.clone();
            match tokio::task::spawn_blocking(move || {
                lumino_export::project::save::save_to_folder(&project, path_clone)
            })
            .await
            {
                Ok(Ok(())) => {
                    cb("工程导出成功", 1.0);
                    tracing::info!("工程导出成功: {:?}", save_path);
                }
                Ok(Err(e)) => {
                    let msg = format!("导出失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
                Err(e) => {
                    let msg = format!("导出任务失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
            }
        });
    }

    /// 处理音频导出
    pub(super) fn handle_audio_export(&mut self) {
        // 获取当前配置
        let config = self.window_state.storage.config.get();
        let soundfont_path = config.ui.soundfont_path.clone();

        // 检查是否有音色库
        if soundfont_path.is_empty() {
            tracing::warn!("没有设置音色库路径，无法导出音频");
            // TODO: 显示错误对话框
            return;
        }

        // 场景1: 没有打开 MIDI 文件
        if self.midi_state.current_midi.is_none()
            && self.midi_state.current_midi_source.is_none()
            && self.midi_state.current_dms.is_none()
        {
            // 检查工作区是否为脏（有编辑内容）
            let has_notes = {
                let ui = self.window_state.window.ui();
                ui.get_editor_note_count() > 0
            };

            if has_notes {
                // 工作区有内容但没有打开 MIDI，先自动保存为 MIDI 文件
                tracing::info!("工作区有内容但没有打开 MIDI，先保存再导出音频");
                if self.save_editor_as_midi_file().is_none() {
                    return; // 用户取消保存
                }
                // 保存成功后 current_midi_source 已被设置，继续后续逻辑
            } else {
                tracing::warn!("没有可导出的内容");
                return;
            }
        }

        // 场景2: 打开了 MIDI 文件
        if let Some(parsed_midi) = &self.midi_state.current_midi {
            let midi_path = parsed_midi.info.path.clone();
            let project_name = get_file_stem(&midi_path);

            // 检查是否有额外的编辑内容
            let ui = self.window_state.window.ui();
            let has_extra_edits = ui.has_notes_changed();

            let output_path = if has_extra_edits {
                // 有额外编辑，需要导出为新的 MIDI 文件
                let file_stem = get_file_stem(&midi_path);
                let output_dir = midi_path.parent().unwrap_or_else(|| Path::new("."));
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                output_dir
                    .join(format!("{}_export_{}.wav", file_stem, timestamp))
                    .to_string_lossy()
                    .to_string()
            } else {
                // 没有额外编辑，复用原路径（改为 .wav 扩展名）
                let mut output = midi_path.clone();
                output.set_extension("wav");
                output.to_string_lossy().to_string()
            };

            // 打开音频导出对话框
            self.window_state.dialog_manager.open_audio_export(
                project_name,
                midi_path.to_string_lossy().to_string(),
                soundfont_path,
                output_path,
            );
            return;
        }

        // 场景3: 打开了 DMS 文件
        if let Some(_parsed_dms) = &self.midi_state.current_dms {
            // DMS 文件需要先转换为 MIDI
            // TODO: 实现 DMS 到 MIDI 的转换
            tracing::info!("DMS 文件导出音频功能待实现");
            return;
        }

        // 场景4: 有源路径但没有完整文档
        if let Some(source_path) = &self.midi_state.current_midi_source {
            let project_name = get_file_stem(source_path);
            let output_path = {
                let mut output = source_path.clone();
                output.set_extension("wav");
                output.to_string_lossy().to_string()
            };

            // 打开音频导出对话框
            self.window_state.dialog_manager.open_audio_export(
                project_name,
                source_path.to_string_lossy().to_string(),
                soundfont_path,
                output_path,
            );
        }
    }

    /// 将 MIDI 数据导入到编辑器
    pub(super) fn import_midi_to_editor(&mut self, parsed: &ParsedMidi) {
        {
            let ui = self.window_state.window.ui_mut();
            self.midi_state
                .midi_handler
                .import_midi_to_editor(ui, parsed);
        }

        // MIDI 导入后，为播放管理器绑定一个独立的 MIDI 输出连接
        if let Some(output) = self.midi_state.midi.create_additional_output() {
            self.window_state
                .window
                .ui_mut()
                .set_playback_midi_output(output);
            tracing::info!("Playback MIDI output connected");
        } else {
            tracing::warn!("Failed to create playback MIDI output connection");
        }
    }

    /// 将 DMS 数据导入到编辑器
    pub(super) fn import_dms_to_editor(&mut self, parsed: &Arc<lumino_midi_loader::ParsedDms>) {
        tracing::info!("[DMS导入] 开始将 DMS 导入编辑器: {:?}", parsed.info.path);

        // DMS 需要先转换为 MIDI 格式才能导入编辑器
        // 使用 load_parsed_midi_from_bytes 直接从字节加载，避免临时文件
        let path = parsed.info.path.clone();
        let track_count = parsed.info.track_count;

        tokio::spawn(async move {
            tracing::info!("[DMS导入] 步骤1: 导出 DMS 为 MIDI");
            match lumino_export::export_midi_from_dms_sync(&path) {
                Ok(midi_bytes) => {
                    tracing::info!(
                        "[DMS导入] 步骤2: DMS 转换为 MIDI 成功，共 {} 字节",
                        midi_bytes.len()
                    );

                    tracing::info!("[DMS导入] 步骤3: 直接加载 MIDI 字节");
                    match lumino_midi_loader::loader::load_parsed_midi_from_bytes(
                        midi_bytes,
                        track_count as u16,
                        0,
                        None,
                    )
                    .await
                    {
                        Ok(parsed_midi) => {
                            tracing::info!(
                                "[DMS导入] 步骤4: DMS 转换的 MIDI 加载成功, 轨道数={}",
                                parsed_midi.info.track_count
                            );
                            tracing::info!("[DMS导入] 步骤5: 发送 MidiParsed 事件");
                            event!(Menu.File.MidiParsed(std::sync::Arc::new(parsed_midi)));
                            tracing::info!("[DMS导入] 事件发送完成");
                        }
                        Err(e) => {
                            tracing::error!("[DMS导入] 加载转换后的 MIDI 失败: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("[DMS导入] DMS 转换为 MIDI 失败: {}", e);
                    event!(
                        Menu.File
                            .DmsParseError(format!("DMS 转换为 MIDI 失败: {e}"))
                    );
                }
            }
        });
    }

    /// 导入后立即输出内存日志（此时尚未触发首帧渲染，能看到干净的后导入态）
    fn log_memory_usage_after_import(&self) {
        if !self.test_state.log_memory_usage {
            return;
        }
        let mem = self.window_state.window.ui().memory_breakdown();
        let rss_mb =
            lumino_memory_monitor::MemoryMonitor::global().current_rss() / (1024 * 1024);
        let front_total = mem.note_instances_front_cap as u64 * mem.note_instance_size as u64;
        let back_total = mem.note_instances_back_cap as u64 * mem.note_instance_size as u64;
        tracing::info!(
            "\n\
            ┌─ Memory Usage (post-import, pre-render) ──────────────┐\n\
            │ 进程 RSS:              {:>8} MB                         │\n\
            ├─────────────────────────────────────────────────────────┤\n\
            │ MidiDocument.events:   {:>8} MB  (Vec<CompactEvent>)    │\n\
            │ editor.notes:          {:>8} MB  (im::Vector<Note>)     │\n\
            │ track_notes({}条):  {:>8} MB  ({} 音符)             │\n\
            │ track_midi_events:     {:>8} MB  ({} 条)               │\n\
            │ onion_skin_cache:      {:>8} MB                         │\n\
            ├─────────────────────────────────────────────────────────┤\n\
            │ note_instances(双缓冲):                                │\n\
            │   前缓冲区:            {:>8} MB  (cap={}, len={})      │\n\
            │   后缓冲区:            {:>8} MB  (cap={}, len={})      │\n\
            │   双缓冲合计:          {:>8} MB                         │\n\
            └─────────────────────────────────────────────────────────┘",
            rss_mb,
            mem.editor.document_events_bytes / (1024 * 1024),
            mem.editor.notes_bytes / (1024 * 1024),
            mem.editor.track_notes_entries,
            mem.editor.track_notes_bytes / (1024 * 1024),
            mem.editor.track_notes_count,
            mem.track_midi_events_bytes / (1024 * 1024),
            mem.track_midi_events_entries,
            mem.cached_onion_skin_bytes / (1024 * 1024),
            front_total / (1024 * 1024),
            mem.note_instances_front_cap,
            mem.note_instances_front_len,
            back_total / (1024 * 1024),
            mem.note_instances_back_cap,
            mem.note_instances_back_len,
            (front_total + back_total) / (1024 * 1024),
        );
    }
}
