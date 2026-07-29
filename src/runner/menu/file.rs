//! Runner 文件菜单处理

mod editor_midi;
mod export;
mod helpers;
mod load;
mod save;

use crate::runner::RunnerInner;

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

                // MIDI 加载后强制使用 Random 调色板并锁定（禁止用户修改）
                lumino_core::palette::set_current_palette_by_name("Random");
                lumino_core::palette::lock_palette();

                // 先导入音符到编辑器（新的懒加载模式：只加载当前音轨，其他音轨按需加载）
                self.import_midi_to_editor(&parsed);

                tracing::debug!("MIDI 文档已导入编辑器，MidiDocument 保留供懒加载使用");

                self.log_memory_usage_after_import();

                // 保留 current_midi 使 MidiDocument 存活（编辑器通过 Arc 引用它做懒加载）
                // 不再需要保存一份全量 track_notes，所以总内存从 (events+notes) 降到 (events)
                self.midi_state.current_midi_source =
                    Some(std::path::PathBuf::from(&parsed.info.path));
                self.midi_state.current_midi = Some(parsed);

                // 设置工程创建时间（从文件系统获取）
                self.session_tracker.created_at = self
                    .midi_state
                    .current_midi_source
                    .as_ref()
                    .and_then(|p| format_created_at_from_path(p));
                if self.session_tracker.created_at.is_some() {
                    tracing::info!(
                        "工程创建时间已设置: {}",
                        self.session_tracker.created_at.as_deref().unwrap_or("")
                    );
                }

                // 启动洋葱皮概览贴图后台生成
                // 先 clone Arc 释放 self 的不可变借用，再调可变方法
                let midi_for_onion = self.midi_state.current_midi.clone();
                if let Some(parsed) = midi_for_onion {
                    self.trigger_onion_skin_generation(&parsed);
                }

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
            Close => {
                self.midi_state.current_midi = None;
                self.midi_state.current_midi_source = None;
                lumino_core::palette::unlock_palette();
                self.window_state.window.ui_mut().dispose_hires_onion_skin();
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

                // 计算真实的创建时间和累计编辑时间
                let created_display = self.session_tracker.created_at.clone().unwrap_or_default();
                let total_editing_time_seconds = self.session_tracker.current_editing_secs();

                // 从编辑器获取当前 BPM
                let tempo = {
                    let ui = self.window_state.window.ui();
                    let root = ui.root();
                    root.editor
                        .editor_state
                        .data
                        .tempo_points
                        .first()
                        .map(|tp| format!("{:.1}", tp.bpm))
                        .unwrap_or_else(|| "120.0".to_string())
                };

                // 将真实数据设置到 UI 状态中
                self.window_state.window.ui_mut().set_project_settings_data(
                    display_title.clone(),
                    tempo,
                    String::new(), // copyright 保持默认
                    created_display,
                    total_editing_time_seconds,
                );

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
                    .set_current_track(track_idx, true);
            }
            ExportProjectArchive => {
                self.handle_export_project_archive();
            }
            ExportProjectFolder => {
                self.handle_export_project_folder();
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
        lumino_core::palette::unlock_palette();

        // 清空编辑器
        self.window_state.window.ui_mut().clear_editor();

        tracing::info!("已创建新工程");
    }

    /// 构建 onion-skin 音符数据并启动后台概览贴图生成
    ///
    /// 单位策略：以 tick 作为时间轴单位（对齐钢琴卷帘的 tick-线性映射），
    /// 因此 `duration_ms` 实为总 tick 数，`OnionSkinNote` 的 start_ms/end_ms 实为 tick。
    fn trigger_onion_skin_generation(&mut self, parsed: &lumino_midi_loader::ParsedMidi) {
        let Some(document) = parsed.document.as_ref() else {
            tracing::debug!("洋葱皮：MIDI 无 document（LMPJ 路径），跳过生成");
            return;
        };

        let total_ticks = document.total_ticks.max(parsed.info.duration_ticks);
        let track_count = document.track_count();
        let mut notes: Vec<Vec<lumino_gfx::OnionSkinNote>> = Vec::with_capacity(track_count);
        let mut editor_track_notes: Vec<Vec<lumino_core::Note>> = Vec::with_capacity(track_count);
        for track_idx in 0..track_count {
            let track_notes = document.track_notes(track_idx);
            let converted: Vec<lumino_gfx::OnionSkinNote> = track_notes
                .iter()
                .map(|n| {
                    lumino_gfx::OnionSkinNote::from_note_event(n, onion_track_color(track_idx))
                })
                .collect();
            notes.push(converted);

            // 同步填充 editor 的 track_notes 缓存，供后续重生成使用
            let editor_notes: Vec<lumino_core::Note> = track_notes
                .iter()
                .map(|n| {
                    lumino_core::Note::from_raw(
                        n.start_tick as f32,
                        n.key as u16,
                        n.length() as f32,
                        n.velocity,
                        n.channel,
                    )
                })
                .collect();
            editor_track_notes.push(editor_notes);
        }

        // 高精度贴图生成
        let key_count = if self.window_state.storage.config.get().ui.enable_256key {
            256
        } else {
            128
        };
        let ppq = parsed.info.division;
        // 轻量 midi_hash：用 total_ticks + track_count + 每轨音符数组合
        let mut hash_input = Vec::new();
        hash_input.extend_from_slice(&total_ticks.to_le_bytes());
        hash_input.extend_from_slice(&(notes.len() as u32).to_le_bytes());
        for track in &notes {
            hash_input.extend_from_slice(&(track.len() as u32).to_le_bytes());
        }
        let midi_hash = lumino_gfx::compute_midi_hash(&hash_input);
        let ui_config = &self.window_state.storage.config.get().ui;
        let config = lumino_gfx::HiResConfig {
            enabled: ui_config.hires_onion_enabled,
            measures_per_group: ui_config.hires_measures_per_group,
            tile_width_px: ui_config.hires_tile_width_px,
            cooldown_secs: ui_config.hires_cooldown_secs,
            gpu_mem_limit_mb: ui_config.hires_gpu_mem_limit_mb,
            render_mode: lumino_gfx::HiResRenderMode::default(),
            group_tile_mem_limit_mb: 256, // 默认值，P2.5 可加设置项
            cache_dir: lumino_gfx::HiResConfig::default().cache_dir, // 用默认缓存目录
        };
        tracing::info!(
            "高精度洋葱皮：启动生成，{} 轨，ppq={}，key_count={}，hash={}",
            notes.len(),
            ppq,
            key_count,
            midi_hash
        );
        // 先预加载 track_notes 缓存，再启动后台生成，
        // 保证后续编辑触发重生成时同组其他音轨数据完整。
        self.window_state
            .window
            .ui_mut()
            .preload_track_notes(editor_track_notes);
        self.window_state.window.ui_mut().generate_hires_onion_skin(
            notes,
            ppq,
            key_count,
            total_ticks,
            config,
            midi_hash,
        );
    }
}

/// 洋葱皮音轨调色板（按音轨索引循环取色）
///
/// 从当前调色板的第二个颜色开始取色（第一个颜色保留给主音轨音符）。
fn onion_track_color(track_idx: usize) -> [u8; 4] {
    lumino_core::palette::onion_track_color(track_idx)
}

/// 从文件路径读取文件创建时间并格式化为本地时间字符串
fn format_created_at_from_path(path: &std::path::Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let created = metadata.modified().ok()?;
    let since_epoch = created
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs() as i64;
    let datetime = chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.with_timezone(&chrono::Local))
        .unwrap_or_else(chrono::Local::now);
    Some(datetime.format("%Y-%m-%d %H:%M:%S").to_string())
}
