//! Runner 文件菜单处理

use std::path::Path;
use std::sync::Arc;

use lumino_core::ParsedMidi;
use lumino_core::event;

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
                tracing::info!("MIDI 文件加载完成：{}", info);
            }
            MidiLoadError(err) => {
                tracing::error!("MIDI 文件加载失败：{}", err);
            }
            MidiParsed(mut parsed) => {
                tracing::info!("MIDI 文件解析完成：{}", parsed.info);

                // 先导入音符到编辑器
                self.import_midi_to_editor(&parsed);

                // 导入完成后释放原始数据
                let _ = parsed.take_midi_data();
                tracing::debug!("MIDI 原始数据已释放，仅保留元数据");

                self.current_midi = Some(parsed);
            }
            MidiParseError(err) => {
                tracing::error!("MIDI 文件解析失败：{}", err);
            }
            DmsParsed(parsed) => {
                tracing::info!("DMS 文件解析完成：{}", parsed.info);

                // 导入 DMS 到编辑器
                self.import_dms_to_editor(&parsed);

                self.current_dms = Some(parsed);
            }
            DmsParseError(err) => {
                tracing::error!("DMS 文件解析失败：{}", err);
            }
            Close => {
                self.current_midi = None;
                self.current_dms = None;
                self.window.ui_mut().clear_editor();
                tracing::info!("工程已关闭");
            }
            TrackSelected(track_idx) => {
                tracing::info!("切换到音轨：{}", track_idx);
                if let Some(memory_manager_arc) = self
                    .current_midi
                    .as_ref()
                    .and_then(|p| p.memory_manager.clone())
                {
                    tracing::debug!("TrackSelected: memory_manager_arc found");
                    if let Ok(mut memory_manager) = memory_manager_arc.lock() {
                        tracing::debug!("TrackSelected: lock acquired");
                        self.midi_handler.load_track_to_editor(
                            &mut memory_manager,
                            track_idx,
                            self.window.ui_mut(),
                        );
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
                tracing::debug!("未处理的文件事件：{:?}", file_event);
            }
        }
    }

    /// 创建新文件
    pub(super) fn handle_new_file(&mut self) {
        // 清空当前工程
        self.current_midi = None;
        self.current_dms = None;

        // 清空编辑器
        self.window.ui_mut().clear_editor();

        tracing::info!("已创建新工程");
    }

    /// 打开文件
    pub(super) fn handle_open_file(&mut self) {
        // 使用 FileHandler 打开文件对话框
        let Some(path) = self.file_handler.handle_open_file() else {
            return;
        };

        let extension = get_file_extension(&path);

        if extension == "dms" {
            self.load_dms_file(path);
        } else {
            self.load_midi_file(path);
        }
    }

    /// 加载 DMS 文件
    pub(super) fn load_dms_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 DMS 文件：{:?}", path);
        let progress_cb = self.progress_cb.clone();
        tokio::spawn(async move {
            run_async_task(
                lumino_core::midi::loader::load_dms(path, Some(&progress_cb)),
                |parsed| event!(Menu.File.DmsParsed(Arc::new(parsed))),
                |e| event!(Menu.File.DmsParseError(e)),
            )
            .await;
        });
    }

    /// 加载 MIDI 文件
    pub(super) fn load_midi_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 MIDI 文件：{:?}", path);
        let progress_cb = self.progress_cb.clone();
        tokio::spawn(async move {
            run_async_task(
                lumino_core::midi::loader::load_parsed_midi(path, Some(&progress_cb)),
                |parsed| event!(Menu.File.MidiParsed(parsed)),
                |e| event!(Menu.File.MidiParseError(e)),
            )
            .await;
        });
    }

    /// 导入文件
    pub(super) fn handle_import_files(&self) {
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

        let extension = get_file_extension(&path);

        tracing::info!("开始后台导入文件：{:?}", path);
        let progress_cb = self.progress_cb.clone();
        tokio::spawn(async move {
            match extension.as_str() {
                "dms" => {
                    run_async_task(
                        lumino_core::midi::loader::load_dms(path, Some(&progress_cb)),
                        |parsed| event!(Menu.File.DmsParsed(Arc::new(parsed))),
                        |e| event!(Menu.File.DmsParseError(e)),
                    )
                    .await;
                }
                _ => {
                    // LMPJ 和 MIDI 都使用 MIDI 加载器
                    run_async_task(
                        lumino_core::midi::loader::load_parsed_midi(path, Some(&progress_cb)),
                        |parsed| event!(Menu.File.MidiParsed(parsed)),
                        |e| event!(Menu.File.MidiParseError(e)),
                    )
                    .await;
                }
            }
        });
    }

    /// 保存文件
    pub(super) fn handle_save_file(&mut self) {
        // 检查是否加载了 MIDI 文件
        if let Some(parsed_midi) = &self.current_midi {
            self.save_midi_file(parsed_midi.clone());
            return;
        }

        // 检查是否加载了 DMS 文件
        if let Some(parsed_dms) = &self.current_dms {
            self.save_dms_file(parsed_dms.clone());
        }
    }

    /// 保存 MIDI 文件
    fn save_midi_file(&self, parsed_midi: ParsedMidi) {
        let file_stem = get_file_stem(Path::new(&parsed_midi.info.path));

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

        let file_service = self.file_service.clone();

        match extension.as_str() {
            "lmpj" => {
                tokio::spawn(async move {
                    let _ = file_service.save_as_lmpj(&parsed_midi, save_path).await;
                });
            }
            "mid" | "midi" => {
                let source_path = parsed_midi.info.path.clone();
                tokio::spawn(async move {
                    let _ = file_service.save_as_midi(source_path, save_path).await;
                });
            }
            "dms" => {
                let source_path = parsed_midi.info.path.clone();
                tokio::spawn(async move {
                    let _ = file_service.save_as_dms(source_path, save_path).await;
                });
            }
            _ => {
                tracing::warn!("不支持的保存格式：{}", extension);
            }
        }
    }

    /// 保存 DMS 文件
    fn save_dms_file(&self, parsed_dms: Arc<lumino_core::ParsedDms>) {
        let file_stem = get_file_stem(Path::new(&parsed_dms.info.path));

        let Some(save_path) = rfd::FileDialog::new()
            .add_filter("Domino 项目", &["dms"])
            .add_filter("MIDI 文件", &["mid", "midi"])
            .set_file_name(format!("{file_stem}.dms"))
            .save_file()
        else {
            return;
        };

        let extension = get_file_extension(&save_path);

        let file_service = self.file_service.clone();
        let source_path = parsed_dms.info.path.clone();

        match extension.as_str() {
            "dms" => {
                tokio::spawn(async move {
                    let _ = file_service.copy_dms_file(source_path, save_path).await;
                });
            }
            "mid" | "midi" => {
                tokio::spawn(async move {
                    let _ = file_service
                        .export_dms_to_midi(source_path, save_path)
                        .await;
                });
            }
            _ => {
                tracing::warn!("不支持的保存格式：{}", extension);
            }
        }
    }

    /// 将 MIDI 数据导入到编辑器
    pub(super) fn import_midi_to_editor(&mut self, parsed: &ParsedMidi) {
        {
            let ui = self.window.ui_mut();
            self.midi_handler.import_midi_to_editor(ui, parsed);
        }

        // MIDI 导入后，为播放管理器绑定一个独立的 MIDI 输出连接
        if let Some(output) = self.midi.create_additional_output() {
            self.window.ui_mut().set_playback_midi_output(output);
            tracing::info!("Playback MIDI output connected");
        } else {
            tracing::warn!("Failed to create playback MIDI output connection");
        }
    }

    /// 将 DMS 数据导入到编辑器
    pub(super) fn import_dms_to_editor(&mut self, parsed: &Arc<lumino_core::ParsedDms>) {
        use lumino_core::midi::loader::{load_parsed_midi, ProgressCallback};

        tracing::info!("[DMS导入] 开始将 DMS 导入编辑器: {:?}", parsed.info.path);

        // DMS 需要先转换为 MIDI 格式才能导入编辑器
        // 这里我们使用 export 功能将 DMS 转为 MIDI 字节，然后解析为 ParsedMidi
        let path = parsed.info.path.clone();

        tokio::spawn(async move {
            tracing::info!("[DMS导入] 步骤1: 导出 DMS 为 MIDI");
            match lumino_export::export_midi_from_dms_sync(&path) {
                Ok(midi_bytes) => {
                    tracing::info!("[DMS导入] 步骤2: DMS 转换为 MIDI 成功，共 {} 字节", midi_bytes.len());

                    // 将转换后的 MIDI 数据保存到临时文件
                    let temp_path = std::env::temp_dir().join("lumino_dms_temp.mid");
                    tracing::info!("[DMS导入] 步骤3: 写入临时文件: {:?}", temp_path);
                    if let Err(e) = std::fs::write(&temp_path, &midi_bytes) {
                        tracing::error!("[DMS导入] 写入临时 MIDI 文件失败: {}", e);
                        return;
                    }
                    tracing::info!("[DMS导入] 步骤4: 临时文件写入成功");

                    // 使用现有的 MIDI 加载器加载转换后的数据
                    let progress_cb: ProgressCallback = Arc::new(|msg: &str, progress: f64| {
                        tracing::info!("[DMS->MIDI] {}: {:.0}%", msg, progress * 100.0);
                    });

                    tracing::info!("[DMS导入] 步骤5: 开始加载 MIDI 数据");
                    match load_parsed_midi(temp_path.clone(), Some(&progress_cb)).await {
                        Ok(parsed_midi) => {
                            tracing::info!("[DMS导入] 步骤6: DMS 转换的 MIDI 加载成功, 轨道数={}", 
                                parsed_midi.info.track_count);
                            // 发送 MIDI 解析完成事件，复用 MIDI 导入逻辑
                            tracing::info!("[DMS导入] 步骤7: 发送 MidiParsed 事件");
                            event!(Menu.File.MidiParsed(parsed_midi));
                            tracing::info!("[DMS导入] 步骤8: MidiParsed 事件发送完成");
                        }
                        Err(e) => {
                            tracing::error!("[DMS导入] 加载转换后的 MIDI 失败: {}", e);
                        }
                    }

                    // 清理临时文件
                    tracing::info!("[DMS导入] 步骤9: 清理临时文件");
                    let _ = std::fs::remove_file(&temp_path);
                    tracing::info!("[DMS导入] 导入流程完成");
                }
                Err(e) => {
                    tracing::error!("[DMS导入] DMS 转换为 MIDI 失败: {}", e);
                }
            }
        });
    }
}
