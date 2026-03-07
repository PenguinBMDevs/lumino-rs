use std::sync::Arc;

use lumino_core::ParsedMidi;
use lumino_core::event;

use super::RunnerInner;

impl RunnerInner {
    pub(super) fn process_core_events(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let events = lumino_core::event::take_events();
        for event in events {
            self.handle_core_event(event_loop, event);
        }
    }

    fn handle_core_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        event: lumino_core::event::Event,
    ) {
        use lumino_core::event::Event;

        match event {
            Event::Menu(menu_event) => {
                self.handle_menu_event(event_loop, menu_event);
            }
            Event::Window(window_event) => {
                self.handle_window_event(window_event);
            }
        }
    }

    fn handle_window_event(&mut self, window_event: lumino_core::event::window::Event) {
        use lumino_core::event::window::Event as WindowEvent;
        use super::dialog_manager::DialogType;
        
        match window_event {
            WindowEvent::OpenCustomPrecisionDialog => {
                tracing::info!("请求打开自定义精度对话框");
                // 打开自定义精度对话框
                self.dialog_manager.open_dialog(DialogType::CustomPrecision);
            }
            WindowEvent::CloseCustomPrecisionDialog => {
                // 关闭对话框
            }
            WindowEvent::ApplyCustomPrecision(_, _) => {
                // 应用精度（在对话框结果中处理）
            }
            _ => {}
        }
    }

    fn handle_menu_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        menu_event: lumino_core::event::menu::Event,
    ) {
        use lumino_core::event::menu::Event::*;

        match menu_event {
            File(file_event) => {
                self.handle_file_menu_event(event_loop, file_event);
            }
            Edit(_) => {
                // 处理编辑事件（占位）
            }
            View(view_event) => {
                self.handle_view_menu_event(view_event);
            }
            Help(_) => {
                // 处理帮助事件（占位）
            }
        }
    }

    fn handle_file_menu_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        file_event: lumino_core::event::menu::file::Event,
    ) {
        use lumino_core::event::menu::file::Event::*;

        match file_event {
            Exit => event_loop.exit(),
            New => self.handle_new_file(),
            Open => self.handle_open_file(),
            ImportFiles => self.handle_import_files(),
            Save => self.handle_save_file(),
            MidiLoaded(info) => {
                tracing::info!("MIDI文件加载完成: {}", info);
            }
            MidiLoadError(err) => {
                tracing::error!("MIDI文件加载失败: {}", err);
            }
            MidiParsed(mut parsed) => {
                tracing::info!("MIDI文件解析完成: {}", parsed.info);
                
                // 先导入音符到编辑器
                self.import_midi_to_editor(&parsed);
                
                // 导入完成后释放原始数据
                let _ = parsed.take_midi_data();
                tracing::debug!("MIDI原始数据已释放，仅保留元数据");

                self.current_midi = Some(parsed);
            }
            MidiParseError(err) => {
                tracing::error!("MIDI文件解析失败: {}", err);
            }
            DmsParsed(parsed) => {
                tracing::info!("DMS 文件解析完成: {}", parsed.info);
                self.current_dms = Some(parsed);
            }
            DmsParseError(err) => {
                tracing::error!("DMS 文件解析失败: {}", err);
            }
            Close => {
                self.current_midi = None;
                self.current_dms = None;
                self.window.ui_mut().clear_editor();
                tracing::info!("工程已关闭");
            }
            TrackSelected(track_idx) => {
                tracing::info!("切换到音轨: {}", track_idx);
                if let Some(memory_manager_arc) = self
                    .current_midi
                    .as_ref()
                    .and_then(|p| p.memory_manager.clone())
                {
                    tracing::debug!("TrackSelected: memory_manager_arc found");
                    if let Ok(mut memory_manager) = memory_manager_arc.lock() {
                        tracing::debug!("TrackSelected: lock acquired");
                        self.load_track_to_editor(&mut memory_manager, track_idx);
                    } else {
                        tracing::warn!("TrackSelected: failed to lock memory_manager");
                    }
                } else {
                    // 没有加载 MIDI 文件，让编辑器处理音轨切换
                    tracing::debug!(
                        "TrackSelected: no MIDI loaded, letting editor handle track switch"
                    );
                    self.window.ui_mut().set_current_track(track_idx);
                }
            }
            _ => {
                tracing::debug!("未处理的文件事件: {:?}", file_event);
            }
        }
    }

    fn handle_new_file(&mut self) {
        // 清空当前工程
        self.current_midi = None;
        self.current_dms = None;

        // 清空编辑器
        self.window.ui_mut().clear_editor();

        tracing::info!("已创建新工程");
    }

    fn handle_open_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("音乐文件", &["mid", "midi", "lmpj", "dms"])
            .add_filter("MIDI 文件", &["mid", "midi"])
            .add_filter("Lumino 项目", &["lmpj"])
            .add_filter("Domino 项目", &["dms"])
            .add_filter("所有文件", &["*"])
            .pick_file()
        else {
            return;
        };

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if extension == "dms" {
            self.load_dms_file(path);
        } else {
            self.load_midi_file(path);
        }
    }

    fn load_dms_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 DMS 文件: {:?}", path);
        tokio::spawn(async move {
            match lumino_core::midi::loader::load_dms(path).await {
                Ok(parsed) => {
                    lumino_core::event::emit(event!(Menu.File.DmsParsed(Arc::new(parsed))));
                }
                Err(e) => {
                    lumino_core::event::emit(event!(Menu.File.DmsParseError(e)));
                }
            }
        });
    }

    fn load_midi_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 MIDI 文件: {:?}", path);
        tokio::spawn(async move {
            match lumino_core::midi::loader::load_parsed_midi(path).await {
                Ok(parsed) => {
                    lumino_core::event::emit(event!(Menu.File.MidiParsed(parsed)));
                }
                Err(e) => {
                    lumino_core::event::emit(event!(Menu.File.MidiParseError(e)));
                }
            }
        });
    }

    fn handle_import_files(&self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("音乐文件", &["mid", "midi", "lmpj", "dms"])
            .add_filter("MIDI 文件", &["mid", "midi"])
            .add_filter("Lumino 项目", &["lmpj"])
            .add_filter("Domino 项目", &["dms"])
            .add_filter("所有文件", &["*"])
            .pick_file()
        else {
            return;
        };

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        tracing::info!("开始后台导入文件: {:?}", path);
        tokio::spawn(async move {
            match extension.as_str() {
                "dms" => match lumino_core::midi::loader::load_dms(path).await {
                    Ok(parsed) => {
                        lumino_core::event::emit(event!(Menu.File.DmsParsed(Arc::new(parsed))));
                    }
                    Err(e) => {
                        lumino_core::event::emit(event!(Menu.File.DmsParseError(e)));
                    }
                },
                _ => {
                    // LMPJ 和 MIDI 都使用 MIDI 加载器
                    match lumino_core::midi::loader::load_parsed_midi(path).await {
                        Ok(parsed) => {
                            lumino_core::event::emit(event!(Menu.File.MidiParsed(parsed)));
                        }
                        Err(e) => {
                            lumino_core::event::emit(event!(Menu.File.MidiParseError(e)));
                        }
                    }
                }
            }
        });
    }

    fn handle_save_file(&mut self) {
        // 检查是否加载了MIDI文件
        if let Some(parsed_midi) = &self.current_midi {
            let file_stem = std::path::Path::new(&parsed_midi.info.path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

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

            let extension = save_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            match extension.as_str() {
                "lmpj" => {
                    let parsed_clone = parsed_midi.clone();
                    let save_path_log = save_path.clone();
                    tokio::spawn(async move {
                        lumino_core::midi::loader::send_progress_message("准备保存 LMPJ 文件", 0.0);
                        lumino_core::midi::loader::send_progress_message("正在保存 LMPJ 文件", 0.3);
                        match lumino_export::save(&parsed_clone, save_path.clone()).await {
                            Ok(()) => {
                                lumino_core::midi::loader::send_progress_message(
                                    "LMPJ 保存成功",
                                    1.0,
                                );
                                tracing::info!("MIDI保存成功: {:?}", save_path_log);
                            }
                            Err(e) => {
                                lumino_core::midi::loader::send_progress_message(
                                    &format!("保存失败: {e}"),
                                    1.0,
                                );
                                tracing::error!("MIDI保存失败: {}", e);
                            }
                        }
                    });
                }
                "mid" | "midi" => {
                    let source_path = parsed_midi.info.path.clone();
                    let save_path_log = save_path.clone();
                    tokio::spawn(async move {
                        lumino_core::midi::loader::send_progress_message("准备导出 MIDI 文件", 0.0);
                        lumino_core::midi::loader::send_progress_message("正在导出 MIDI 文件", 0.3);
                        match tokio::task::spawn_blocking(move || {
                            lumino_export::export_midi_from_parsed_midi_sync(&source_path)
                        })
                        .await
                        {
                            Ok(Ok(bytes)) => {
                                lumino_core::midi::loader::send_progress_message(
                                    "正在写入文件",
                                    0.8,
                                );
                                match std::fs::write(&save_path_log, bytes) {
                                    Ok(()) => {
                                        lumino_core::midi::loader::send_progress_message(
                                            "MIDI 导出成功",
                                            1.0,
                                        );
                                        tracing::info!("MIDI 导出成功");
                                    }
                                    Err(e) => {
                                        lumino_core::midi::loader::send_progress_message(
                                            &format!("写入文件失败: {e}"),
                                            1.0,
                                        );
                                        tracing::error!("MIDI 导出失败: {}", e);
                                    }
                                }
                            }
                            Ok(Err(e)) => {
                                lumino_core::midi::loader::send_progress_message(
                                    &format!("导出失败: {e}"),
                                    1.0,
                                );
                                tracing::error!("MIDI 导出失败: {}", e);
                            }
                            Err(e) => {
                                lumino_core::midi::loader::send_progress_message(
                                    &format!("导出失败: {e}"),
                                    1.0,
                                );
                                tracing::error!("MIDI 导出失败: {}", e);
                            }
                        }
                    });
                }
                "dms" => {
                    let source_path = parsed_midi.info.path.clone();
                    tokio::spawn(async move {
                        lumino_core::midi::loader::send_progress_message("准备导出 DMS 文件", 0.0);
                        lumino_core::midi::loader::send_progress_message("正在读取 MIDI 文件", 0.2);
                        match tokio::task::spawn_blocking(move || {
                            lumino_core::midi::loader::send_progress_message("正在转换格式", 0.5);
                            let bytes = lumino_export::export_dms_from_midi_sync(&source_path)?;
                            lumino_core::midi::loader::send_progress_message(
                                "正在写入 DMS 文件",
                                0.8,
                            );
                            std::fs::write(&save_path, bytes)
                                .map_err(|e| format!("写入文件失败: {e}"))
                        })
                        .await
                        {
                            Ok(Ok(_)) => {
                                lumino_core::midi::loader::send_progress_message(
                                    "MIDI 转 DMS 导出成功",
                                    1.0,
                                );
                                tracing::info!("MIDI 转 DMS 导出成功");
                            }
                            Ok(Err(e)) => {
                                lumino_core::midi::loader::send_progress_message(
                                    &format!("导出失败: {e}"),
                                    1.0,
                                );
                                tracing::error!("MIDI 转 DMS 导出失败: {}", e);
                            }
                            Err(e) => {
                                lumino_core::midi::loader::send_progress_message(
                                    &format!("导出失败: {e}"),
                                    1.0,
                                );
                                tracing::error!("MIDI 转 DMS 导出失败: {}", e);
                            }
                        }
                    });
                }
                _ => {
                    tracing::warn!("不支持的保存格式: {}", extension);
                }
            }
            return;
        }

        // 检查是否加载了DMS文件
        if let Some(parsed_dms) = &self.current_dms {
            let file_stem = std::path::Path::new(&parsed_dms.info.path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let Some(save_path) = rfd::FileDialog::new()
                .add_filter("Domino 项目", &["dms"])
                .add_filter("MIDI 文件", &["mid", "midi"])
                .set_file_name(format!("{file_stem}.dms"))
                .save_file()
            else {
                return;
            };

            let extension = save_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            match extension.as_str() {
                "dms" => {
                    let source_path = parsed_dms.info.path.clone();
                    let save_path_log = save_path.clone();
                    tokio::spawn(async move {
                        lumino_core::midi::loader::send_progress_message("准备保存 DMS 文件", 0.0);
                        lumino_core::midi::loader::send_progress_message("正在复制 DMS 文件", 0.5);
                        match tokio::task::spawn_blocking(move || {
                            lumino_export::copy_file_sync(&source_path, &save_path)
                        })
                        .await
                        {
                            Ok(Ok(_)) => {
                                lumino_core::midi::loader::send_progress_message(
                                    "DMS 保存成功",
                                    1.0,
                                );
                                tracing::info!("DMS 保存成功: {:?}", save_path_log);
                            }
                            Ok(Err(e)) => {
                                lumino_core::midi::loader::send_progress_message(
                                    &format!("保存失败: {e}"),
                                    1.0,
                                );
                                tracing::error!("DMS 保存失败: {}", e);
                            }
                            Err(e) => {
                                lumino_core::midi::loader::send_progress_message(
                                    &format!("保存失败: {e}"),
                                    1.0,
                                );
                                tracing::error!("DMS 保存失败: {}", e);
                            }
                        }
                    });
                }
                "mid" | "midi" => {
                    let source_path = parsed_dms.info.path.clone();
                    tokio::spawn(async move {
                        lumino_core::midi::loader::send_progress_message("准备导出 MIDI 文件", 0.0);
                        lumino_core::midi::loader::send_progress_message("正在读取 DMS 文件", 0.2);
                        match tokio::task::spawn_blocking(move || {
                            let bytes = lumino_export::export_midi_from_dms_sync(&source_path)?;
                            lumino_core::midi::loader::send_progress_message(
                                "正在写入 MIDI 文件",
                                0.8,
                            );
                            std::fs::write(&save_path, bytes)
                                .map_err(|e| format!("写入文件失败: {e}"))
                        })
                        .await
                        {
                            Ok(Ok(_)) => {
                                lumino_core::midi::loader::send_progress_message(
                                    "DMS 转 MIDI 导出成功",
                                    1.0,
                                );
                                tracing::info!("DMS 转 MIDI 导出成功");
                            }
                            Ok(Err(e)) => {
                                lumino_core::midi::loader::send_progress_message(
                                    &format!("导出失败: {e}"),
                                    1.0,
                                );
                                tracing::error!("DMS 转 MIDI 导出失败: {}", e);
                            }
                            Err(e) => {
                                lumino_core::midi::loader::send_progress_message(
                                    &format!("导出失败: {e}"),
                                    1.0,
                                );
                                tracing::error!("DMS 转 MIDI 导出失败: {}", e);
                            }
                        }
                    });
                }
                _ => {
                    tracing::warn!("不支持的保存格式: {}", extension);
                }
            }
            return;
        }

        // 没有加载任何文件，但有编辑器数据，保存为新建工程
        let editor_notes = self.window.ui().get_editor_notes();
        if editor_notes.is_empty() {
            tracing::warn!("没有可保存的内容");
            return;
        }

        let Some(save_path) = rfd::FileDialog::new()
            .add_filter("Lumino MIDI Project", &["lmpj"])
            .add_filter("MIDI 文件 (.mid)", &["mid"])
            .add_filter("MIDI 文件 (.midi)", &["midi"])
            .set_file_name("新建工程.lmpj")
            .save_file()
        else {
            return;
        };

        let extension = save_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        match extension.as_str() {
            "lmpj" => {
                self.save_editor_as_lmpj(editor_notes, save_path);
            }
            "mid" | "midi" => {
                self.save_editor_as_midi(editor_notes, save_path);
            }
            _ => {
                tracing::warn!("不支持的保存格式: {}", extension);
            }
        }
    }

    /// 保存编辑器数据为 LMPJ 文件
    fn save_editor_as_lmpj(
        &self,
        editor_notes: Vec<(usize, Vec<(f32, u8, f32)>)>,
        save_path: std::path::PathBuf,
    ) {
        // 创建 ParsedMidi 数据结构
        let parsed_midi = self.build_parsed_midi_from_editor(&editor_notes, &save_path);

        let save_path_log = save_path.clone();
        tokio::spawn(async move {
            lumino_core::midi::loader::send_progress_message("准备保存 LMPJ 文件", 0.0);
            lumino_core::midi::loader::send_progress_message("正在保存 LMPJ 文件", 0.3);
            match lumino_export::save(&parsed_midi, save_path.clone()).await {
                Ok(()) => {
                    lumino_core::midi::loader::send_progress_message("LMPJ 保存成功", 1.0);
                    tracing::info!("新建工程保存成功: {:?}", save_path_log);
                }
                Err(e) => {
                    lumino_core::midi::loader::send_progress_message(
                        &format!("保存失败: {e}"),
                        1.0,
                    );
                    tracing::error!("新建工程保存失败: {}", e);
                }
            }
        });
    }

    /// 保存编辑器数据为 MIDI 文件
    fn save_editor_as_midi(
        &self,
        editor_notes: Vec<(usize, Vec<(f32, u8, f32)>)>,
        save_path: std::path::PathBuf,
    ) {
        // 构建 MIDI 导出数据
        let midi_data = self.build_midi_export_data(&editor_notes);
        let save_path_log = save_path.clone();

        tokio::spawn(async move {
            lumino_core::midi::loader::send_progress_message("准备导出 MIDI 文件", 0.0);
            lumino_core::midi::loader::send_progress_message("正在导出 MIDI 文件", 0.5);

            match tokio::task::spawn_blocking(move || {
                lumino_export::export_midi_to_bytes(&midi_data)
            })
            .await
            {
                Ok(Ok(bytes)) => {
                    lumino_core::midi::loader::send_progress_message("正在写入文件", 0.8);
                    match std::fs::write(&save_path_log, bytes) {
                        Ok(()) => {
                            lumino_core::midi::loader::send_progress_message("MIDI 导出成功", 1.0);
                            tracing::info!("新建工程导出为 MIDI 成功: {:?}", save_path_log);
                        }
                        Err(e) => {
                            lumino_core::midi::loader::send_progress_message(
                                &format!("写入文件失败: {e}"),
                                1.0,
                            );
                            tracing::error!("MIDI 导出失败: {}", e);
                        }
                    }
                }
                Ok(Err(e)) => {
                    lumino_core::midi::loader::send_progress_message(
                        &format!("导出失败: {e}"),
                        1.0,
                    );
                    tracing::error!("MIDI 导出失败: {}", e);
                }
                Err(e) => {
                    lumino_core::midi::loader::send_progress_message(
                        &format!("导出失败: {e}"),
                        1.0,
                    );
                    tracing::error!("MIDI 导出失败: {}", e);
                }
            }
        });
    }

    /// 从编辑器音符构建 ParsedMidi
    fn build_parsed_midi_from_editor(
        &self,
        editor_notes: &[(usize, Vec<(f32, u8, f32)>)],
        save_path: &std::path::Path,
    ) -> lumino_core::ParsedMidi {
        use lumino_core::midi::info::MidiInfo;

        // 计算总音符数和最大 tick
        let total_notes: u64 = editor_notes
            .iter()
            .map(|(_, notes)| notes.len() as u64)
            .sum();
        let max_tick = editor_notes
            .iter()
            .flat_map(|(_, notes)| notes.iter())
            .map(|(tick, _, length)| tick + length)
            .fold(0.0f32, f32::max) as u32;

        // 生成 MIDI 字节流用于保存
        let midi_export_data = self.build_midi_export_data(editor_notes);
        let midi_bytes = match lumino_export::export_midi_to_bytes(&midi_export_data) {
            Ok(bytes) => {
                tracing::info!("生成 MIDI 字节流成功: {} 字节", bytes.len());
                if !bytes.is_empty() {
                    Some(bytes)
                } else {
                    tracing::warn!("生成的 MIDI 字节流为空");
                    None
                }
            }
            Err(e) => {
                tracing::error!("生成 MIDI 字节流失败: {}", e);
                None
            }
        };

        tracing::info!(
            "构建 ParsedMidi: 音轨数={}, 总音符数={}, midi_data={}",
            editor_notes.len(),
            total_notes,
            if midi_bytes.is_some() { "有" } else { "无" }
        );

        lumino_core::ParsedMidi {
            info: MidiInfo {
                path: save_path.to_path_buf(),
                track_count: editor_notes.len() as u16,
                total_notes,
                duration_ticks: max_tick,
                division: 1920,
                parse_progress: None,
            },
            midi_data: midi_bytes,
            memory_manager: None,
        }
    }

    /// 从编辑器音符构建 MIDI 导出数据
    fn build_midi_export_data(
        &self,
        editor_notes: &[(usize, Vec<(f32, u8, f32)>)],
    ) -> lumino_export::midi::MidiExportData {
        use lumino_export::midi::{
            MidiExportData, MidiExportOptions, MidiNoteEvent, MidiTrackData,
        };

        let mut tracks = Vec::new();

        for (track_idx, notes) in editor_notes {
            let track_notes: Vec<MidiNoteEvent> = notes
                .iter()
                .map(|(tick, key, length)| MidiNoteEvent {
                    tick: *tick as u32,
                    channel: 0,
                    key: *key,
                    velocity: 100,
                    duration: *length as u32,
                })
                .collect();

            let track_data = MidiTrackData {
                notes: track_notes,
                tempos: vec![],
                program_changes: vec![],
                control_changes: vec![],
                time_signatures: vec![],
                key_signatures: vec![],
                name: Some(format!("Track {}", track_idx + 1)),
            };

            tracks.push(track_data);
        }

        MidiExportData {
            options: MidiExportOptions {
                format: 1,
                ppqn: 1920,
            },
            tracks,
        }
    }

    fn handle_view_menu_event(&mut self, view_event: lumino_core::event::menu::view::Event) {
        use lumino_core::event::menu::view::Event::*;

        match view_event {
            Theme(theme) => {
                self.window.ui_mut().update_theme(theme.clone());
                self.storage.config.patch(|state| {
                    state.ui.theme = theme;
                });
            }
        }
    }

    /// 将 MIDI 数据导入到编辑器
    fn import_midi_to_editor(&mut self, parsed: &ParsedMidi) {
        use lumino_core::MidiEvent;

        // 获取 memory_manager
        if let Some(memory_manager_arc) = parsed.memory_manager.as_ref() {
            // 有 memory_manager，使用原有逻辑
            let mut memory_manager: std::sync::MutexGuard<lumino_core::MidiMemoryManager> =
                match memory_manager_arc.lock() {
                    Ok(mgr) => mgr,
                    Err(e) => {
                        tracing::error!("无法锁定 memory_manager: {}", e);
                        return;
                    }
                };

            // 收集所有音轨信息
            let mut track_infos = Vec::new();
            let summaries = memory_manager.all_summaries().to_vec();

            for summary in &summaries {
                let track_idx = summary.track_index;

                // 获取音轨事件以读取音轨名称
                let track_name: Option<String> =
                    match memory_manager.get_track_events_full(track_idx) {
                        Ok(events) => {
                            // 查找 TrackName 事件
                            events.iter().find_map(|e: &MidiEvent| {
                                if let MidiEvent::TrackName { name, .. } = e {
                                    Some(name.clone())
                                } else {
                                    None
                                }
                            })
                        }
                        Err(e) => {
                            tracing::warn!("无法获取音轨 {} 事件: {}", track_idx, e);
                            None
                        }
                    };

                track_infos.push((track_idx, track_name, summary.note_count));
            }

            // 更新 UI 音轨列表
            self.window.ui_mut().update_tracks(&track_infos);

            // 预加载所有音轨的音符到 track_notes（供洋葱皮使用）
            tracing::info!("Pre-loading all tracks for onion skin...");
            for (track_idx, _, note_count) in &track_infos {
                if *note_count > 0 {
                    // 加载音符但不切换到该音轨（只保存到 track_notes）
                    self.preload_track_for_onion_skin(&mut memory_manager, *track_idx);
                }
            }

            // 加载第一个有音符的音轨到编辑器（实际显示）
            if let Some((first_track_idx, _, _)) = track_infos
                .iter()
                .find(|(_, _, note_count)| *note_count > 0)
            {
                self.load_track_to_editor(&mut memory_manager, *first_track_idx);
            }
        } else if let Some(midi_data) = parsed.midi_data.as_ref() {
            // 没有 memory_manager 但有 midi_data，从 midi_data 解析音符
            tracing::info!("从 midi_data 解析音符数据");
            self.import_midi_data_to_editor(midi_data, parsed.info.track_count as usize);
        } else {
            tracing::warn!("MIDI 没有 memory_manager 也没有 midi_data，无法导入音符");
            return;
        }

        // 更新编辑器总 ticks
        let total_ticks = parsed.info.duration_ticks as f32;
        self.window.ui_mut().set_total_ticks(total_ticks);
    }

    /// 从 MIDI 字节流导入音符到编辑器
    fn import_midi_data_to_editor(&mut self, midi_data: &[u8], _track_count: usize) {
        use midly::{Smf, TrackEventKind};

        // 解析 MIDI 数据
        let smf = match Smf::parse(midi_data) {
            Ok(smf) => smf,
            Err(e) => {
                tracing::error!("解析 MIDI 数据失败: {}", e);
                return;
            }
        };

        // 收集音轨信息
        let mut track_infos = Vec::new();
        let mut track_notes_map: std::collections::HashMap<usize, Vec<(f32, u8, f32)>> =
            std::collections::HashMap::new();

        for (track_idx, track) in smf.tracks.iter().enumerate() {
            let mut active_notes: std::collections::HashMap<(u8, u8), u32> =
                std::collections::HashMap::new();
            let mut notes = Vec::new();
            let mut track_name: Option<String> = None;
            let mut abs_tick: u32 = 0;

            for event in track {
                abs_tick += u32::from(event.delta);

                match event.kind {
                    TrackEventKind::Meta(midly::MetaMessage::TrackName(name_bytes)) => {
                        track_name = String::from_utf8(name_bytes.to_vec()).ok();
                    }
                    TrackEventKind::Midi {
                        channel,
                        message: midly::MidiMessage::NoteOn { key, vel },
                    } => {
                        if vel > 0 {
                            active_notes.insert((channel.as_int(), key.as_int()), abs_tick);
                        } else {
                            // velocity == 0 视为 NoteOff
                            if let Some(start_tick) =
                                active_notes.remove(&(channel.as_int(), key.as_int()))
                            {
                                let length = abs_tick.saturating_sub(start_tick) as f32;
                                notes.push((start_tick as f32, key.as_int(), length));
                            }
                        }
                    }
                    TrackEventKind::Midi {
                        channel,
                        message: midly::MidiMessage::NoteOff { key, .. },
                    } => {
                        if let Some(start_tick) =
                            active_notes.remove(&(channel.as_int(), key.as_int()))
                        {
                            let length = abs_tick.saturating_sub(start_tick) as f32;
                            notes.push((start_tick as f32, key.as_int(), length));
                        }
                    }
                    _ => {}
                }
            }

            // 处理未关闭的音符
            let track_end_tick = abs_tick;
            for ((_channel, key), start_tick) in active_notes {
                let length = track_end_tick.saturating_sub(start_tick) as f32;
                notes.push((start_tick as f32, key, length));
            }

            if !notes.is_empty() {
                track_notes_map.insert(track_idx, notes.clone());
            }

            track_infos.push((track_idx, track_name, notes.len() as u64));
        }

        // 更新 UI 音轨列表
        self.window.ui_mut().update_tracks(&track_infos);

        // 将所有音轨的音符导入编辑器
        for (track_idx, notes) in track_notes_map {
            self.window.ui_mut().load_track_notes(track_idx, &notes);
            tracing::info!(
                "从 midi_data 导入音轨 {}，共 {} 个音符",
                track_idx,
                notes.len()
            );
        }

        // 加载第一个有音符的音轨到编辑器（实际显示）
        if let Some((first_track_idx, _, _)) = track_infos
            .iter()
            .find(|(_, _, note_count)| *note_count > 0)
        {
            self.window.ui_mut().set_current_track(*first_track_idx);
        }
    }

    /// 加载指定音轨的音符到编辑器
    fn load_track_to_editor(
        &mut self,
        memory_manager: &mut lumino_core::MidiMemoryManager,
        track_idx: usize,
    ) {
        use lumino_core::MidiEvent;

        tracing::info!("load_track_to_editor: track_idx={}", track_idx);

        let events = match memory_manager.get_track_events_full(track_idx) {
            Ok(events) => {
                tracing::info!("  got {} events from track {}", events.len(), track_idx);
                // 统计音符事件
                let note_on_count = events
                    .iter()
                    .filter(|e| matches!(e, MidiEvent::NoteOn { .. }))
                    .count();
                let note_off_count = events
                    .iter()
                    .filter(|e| matches!(e, MidiEvent::NoteOff { .. }))
                    .count();
                tracing::info!("  NoteOn: {}, NoteOff: {}", note_on_count, note_off_count);
                events
            }
            Err(e) => {
                tracing::error!("加载音轨 {} 失败: {}", track_idx, e);
                return;
            }
        };

        // 构建音符列表（配对 NoteOn 和 NoteOff）
        let mut active_notes: std::collections::HashMap<(u8, u8), u32> =
            std::collections::HashMap::new();
        let mut notes = Vec::new();

        for event in &events {
            match event {
                MidiEvent::NoteOn {
                    track: _,
                    tick,
                    channel,
                    key,
                    velocity,
                } => {
                    if *velocity > 0 {
                        // 记录音符开始
                        active_notes.insert((*channel, *key), *tick);
                    } else if let Some(start_tick) = active_notes.remove(&(*channel, *key)) {
                        // velocity == 0 视为 NoteOff
                        let length = tick.saturating_sub(start_tick) as f32;
                        notes.push((start_tick as f32, *key, length));
                    }
                }
                MidiEvent::NoteOff {
                    track: _,
                    tick,
                    channel,
                    key,
                    ..
                } => {
                    if let Some(start_tick) = active_notes.remove(&(*channel, *key)) {
                        let length = tick.saturating_sub(start_tick) as f32;
                        notes.push((start_tick as f32, *key, length));
                    }
                }
                _ => {}
            }
        }

        // 处理未关闭的音符（到音轨结束）
        let track_end_tick = events.iter().map(|e| e.tick()).max().unwrap_or(0);
        for ((_channel, key), start_tick) in active_notes {
            let length = track_end_tick.saturating_sub(start_tick) as f32;
            notes.push((start_tick as f32, key, length));
        }

        // 更新编辑器音符（使用新的函数，同时保存到 track_notes 供洋葱皮使用）
        self.window.ui_mut().load_track_notes(track_idx, &notes);

        tracing::info!("音轨 {} 已加载，共 {} 个音符", track_idx, notes.len());
    }

    /// 预加载音轨音符到 track_notes（用于洋葱皮，不切换到该音轨）
    fn preload_track_for_onion_skin(
        &mut self,
        memory_manager: &mut lumino_core::MidiMemoryManager,
        track_idx: usize,
    ) {
        use lumino_core::MidiEvent;

        tracing::debug!("Preloading track {} for onion skin", track_idx);

        let events = match memory_manager.get_track_events_full(track_idx) {
            Ok(events) => events,
            Err(e) => {
                tracing::warn!("预加载音轨 {} 失败: {}", track_idx, e);
                return;
            }
        };

        // 构建音符列表（配对 NoteOn 和 NoteOff）
        let mut active_notes: std::collections::HashMap<(u8, u8), u32> =
            std::collections::HashMap::new();
        let mut notes = Vec::new();

        for event in &events {
            match event {
                MidiEvent::NoteOn {
                    track: _,
                    tick,
                    channel,
                    key,
                    velocity,
                } => {
                    if *velocity > 0 {
                        active_notes.insert((*channel, *key), *tick);
                    } else if let Some(start_tick) = active_notes.remove(&(*channel, *key)) {
                        let length = tick.saturating_sub(start_tick) as f32;
                        notes.push((start_tick as f32, *key, length));
                    }
                }
                MidiEvent::NoteOff {
                    track: _,
                    tick,
                    channel,
                    key,
                    ..
                } => {
                    if let Some(start_tick) = active_notes.remove(&(*channel, *key)) {
                        let length = tick.saturating_sub(start_tick) as f32;
                        notes.push((start_tick as f32, *key, length));
                    }
                }
                _ => {}
            }
        }

        // 处理未关闭的音符
        let track_end_tick = events.iter().map(|e| e.tick()).max().unwrap_or(0);
        for ((_channel, key), start_tick) in active_notes {
            let length = track_end_tick.saturating_sub(start_tick) as f32;
            notes.push((start_tick as f32, key, length));
        }

        // 只保存到 track_notes，不切换到该音轨
        if !notes.is_empty() {
            self.window
                .ui_mut()
                .load_track_notes_for_onion_skin(track_idx, &notes);
            tracing::debug!(
                "Preloaded track {} with {} notes for onion skin",
                track_idx,
                notes.len()
            );
        }
    }
}
