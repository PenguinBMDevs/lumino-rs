use std::sync::Arc;

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
            Event::Window(_) => {}
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
                tracing::info!("工程已关闭");
            }
            _ => {
                tracing::debug!("未处理的文件事件: {:?}", file_event);
            }
        }
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

        tracing::warn!("没有加载的文件，无法保存");
    }

    fn handle_view_menu_event(&mut self, view_event: lumino_core::event::menu::view::Event) {
        use lumino_core::event::menu::view::Event::*;

        match view_event {
            Theme(theme) => {
                self.ui.update_theme(theme.clone());
                self.storage.config.patch(|state| {
                    state.ui.theme = theme;
                });
            }
        }
    }
}
