//! Runner 文件菜单处理

use std::sync::Arc;

use lumino_core::ParsedMidi;
use lumino_core::event;

use crate::runner::{RunnerInner, async_helper::run_async_task};

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
                tracing::debug!("未处理的文件事件: {:?}", file_event);
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

    /// 加载 DMS 文件
    pub(super) fn load_dms_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 DMS 文件: {:?}", path);
        tokio::spawn(async move {
            run_async_task(
                lumino_core::midi::loader::load_dms(path),
                |parsed| event!(Menu.File.DmsParsed(Arc::new(parsed))),
                |e| event!(Menu.File.DmsParseError(e)),
            )
            .await;
        });
    }

    /// 加载 MIDI 文件
    pub(super) fn load_midi_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 MIDI 文件: {:?}", path);
        tokio::spawn(async move {
            run_async_task(
                lumino_core::midi::loader::load_parsed_midi(path),
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

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        tracing::info!("开始后台导入文件: {:?}", path);
        tokio::spawn(async move {
            match extension.as_str() {
                "dms" => {
                    run_async_task(
                        lumino_core::midi::loader::load_dms(path),
                        |parsed| event!(Menu.File.DmsParsed(Arc::new(parsed))),
                        |e| event!(Menu.File.DmsParseError(e)),
                    )
                    .await;
                }
                _ => {
                    // LMPJ 和 MIDI 都使用 MIDI 加载器
                    run_async_task(
                        lumino_core::midi::loader::load_parsed_midi(path),
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
        // 检查是否加载了MIDI文件
        if let Some(parsed_midi) = &self.current_midi {
            self.save_midi_file(parsed_midi.clone());
            return;
        }

        // 检查是否加载了DMS文件
        if let Some(parsed_dms) = &self.current_dms {
            self.save_dms_file(parsed_dms.clone());
        }
    }

    /// 保存 MIDI 文件
    fn save_midi_file(&self, parsed_midi: ParsedMidi) {
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
                tracing::warn!("不支持的保存格式: {}", extension);
            }
        }
    }

    /// 保存 DMS 文件
    fn save_dms_file(&self, parsed_dms: Arc<lumino_core::midi::loader::ParsedDms>) {
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
                tracing::warn!("不支持的保存格式: {}", extension);
            }
        }
    }

    /// 将 MIDI 数据导入到编辑器
    pub(super) fn import_midi_to_editor(&mut self, parsed: &ParsedMidi) {
        self.midi_handler
            .import_midi_to_editor(self.window.ui_mut(), parsed);
    }
}
