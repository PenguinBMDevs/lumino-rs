//! Runner 文件菜单：工程导出与音频导出

use std::path::Path;
use std::sync::Arc;

use crate::runner::RunnerInner;

use super::helpers::get_file_stem;

impl RunnerInner {
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

    /// 检查工作区是否需要先自动保存（场景1）
    /// 返回 true 表示可继续，false 表示已中止
    fn try_save_workspace_if_needed(&mut self) -> bool {
        if self.midi_state.current_midi.is_some()
            || self.midi_state.current_midi_source.is_some()
            || self.midi_state.current_dms.is_some()
        {
            return true; // 已有文件，不需要保存
        }

        let has_notes = {
            let ui = self.window_state.window.ui();
            ui.get_editor_note_count() > 0
        };

        if has_notes {
            tracing::info!("工作区有内容但没有打开 MIDI，先保存再导出音频");
            self.save_editor_as_midi_file().is_some() // 保存成功才继续
        } else {
            tracing::warn!("没有可导出的内容");
            false
        }
    }

    /// 为已有 MIDI 文档构建音频导出路径（场景2）
    /// 有额外编辑时生成独立时间戳文件名，无编辑时复用 MIDI 路径的 .wav
    fn build_export_path_for_midi(&self, midi_path: &Path) -> (String, String) {
        let project_name = get_file_stem(midi_path);

        let ui = self.window_state.window.ui();
        let has_extra_edits = ui.has_notes_changed();

        let output_path = if has_extra_edits {
            let file_stem = get_file_stem(midi_path);
            let output_dir = midi_path.parent().unwrap_or(Path::new("."));
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            output_dir
                .join(format!("{}_export_{}.wav", file_stem, timestamp))
                .to_string_lossy()
                .to_string()
        } else {
            let mut output = midi_path.to_path_buf();
            output.set_extension("wav");
            output.to_string_lossy().to_string()
        };

        (project_name, output_path)
    }

    /// 为源路径（没有完整文档）构建音频导出路径（场景4）
    fn build_export_path_for_source(&self, source_path: &Path) -> (String, String) {
        let project_name = get_file_stem(source_path);
        let mut output = source_path.to_path_buf();
        output.set_extension("wav");
        (project_name, output.to_string_lossy().to_string())
    }

    /// 打开音频导出对话框（公共方法）
    /// 从主窗口面板直接启动音频导出（带完整参数）
    pub(super) fn handle_audio_export_start(
        &mut self,
        project_name: String,
        midi_path: String,
        soundfont_path: String,
        output_path: String,
        sample_rate: u32,
        channels: u16,
        layers: u32,
        apply_limiter: bool,
        disable_fade_out: bool,
        linear_envelope: bool,
        format: u8,
    ) {
        let sf2 = std::path::PathBuf::from(&soundfont_path);
        let output = std::path::PathBuf::from(&output_path);

        // 验证音色库
        if !sf2.exists() {
            tracing::error!("音色库文件不存在: {:?}", sf2);
            return;
        }

        // 构造导出选项
        let options = lumino_export::audio::AudioExportOptions {
            sample_rate,
            channels: if channels == 1 {
                lumino_export::audio::AudioChannels::Mono
            } else {
                lumino_export::audio::AudioChannels::Stereo
            },
            layers,
            channel_threading: lumino_export::audio::ThreadingOption::Auto,
            key_threading: lumino_export::audio::ThreadingOption::Auto,
            apply_limiter,
            disable_fade_out,
            linear_envelope,
            interpolation: lumino_export::audio::Interpolation::default(),
            format: if format == 0 {
                lumino_export::audio::AudioFormat::WAV
            } else {
                lumino_export::audio::AudioFormat::FLAC
            },
        };

        let cb = self.window_state.progress_cb.clone();
        let midi_path_buf = std::path::PathBuf::from(&midi_path);
        let midi_on_disk = midi_path_buf.exists();
        let parsed_midi = self.midi_state.current_midi.clone();
        let output_display = output_path.clone();

        tracing::info!(
            "开始音频导出(面板): project={}, midi={}, sf2={}, output={}",
            project_name,
            midi_path,
            soundfont_path,
            output_path,
        );

        tokio::spawn(async move {
            cb("正在导出音频...", 0.0);
            let cb_inner = cb.clone();

            let result = tokio::task::spawn_blocking(move || {
                if let Some(pm) = parsed_midi {
                    // 路径 A：内存已有 MidiDocument
                    lumino_export::audio::export_audio_from_parsed(
                        &pm,
                        &sf2,
                        &output,
                        &options,
                        Some(Arc::new(move |p| {
                            cb_inner(
                                &format!("导出中... {:.0}%", p as f64),
                                p as f64 / 100.0,
                            );
                        })),
                        None,
                    )
                } else if midi_on_disk {
                    // 路径 B：流式渲染（零事件常驻）
                    let bytes = std::fs::read(&midi_path_buf)
                        .map_err(lumino_export::ExportError::Io)?;
                    lumino_export::audio::render_streaming_gpu(
                        &bytes,
                        &sf2,
                        &output,
                        &options,
                        Some(Arc::new(move |p| {
                            cb_inner(
                                &format!("导出中... {:.0}%", p as f64),
                                p as f64 / 100.0,
                            );
                        })),
                        None,
                    )
                } else {
                    Err(lumino_export::ExportError::InvalidData(
                        "既无内存 MIDI 数据，也无 MIDI 文件路径，无法导出".to_string(),
                    ))
                }
            })
            .await;

            match result {
                Ok(Ok(())) => {
                    cb("音频导出成功", 1.0);
                    tracing::info!("音频导出成功: {:?}", output_display);
                }
                Ok(Err(e)) => {
                    let msg = format!("音频导出失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
                Err(e) => {
                    let msg = format!("音频导出任务失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
            }
        });
    }

    /// 处理音频导出（菜单触发 → 切换到音频导出面板）
    pub(super) fn handle_audio_export(&mut self) {
        tracing::info!("音频导出已迁移到侧边栏面板，请点击侧边栏的音频导出按钮");
    }
}
