//! 对话框事件处理模块

use std::sync::Arc;

use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use lumino_ui::state::root_state::{
    AudioChannels as UiAudioChannels, AudioFormat as UiAudioFormat,
    Interpolation as UiInterpolation, ThreadingOption as UiThreadingOption,
};

use crate::runner::dialog_manager::DialogResult;
use crate::runner::inner::RunnerInner;

impl RunnerInner {
    /// 处理对话框窗口事件
    pub(crate) fn handle_dialog_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) -> bool {
        // 检查是否是对话框窗口
        if !self.window_state.dialog_manager.is_dialog_window(window_id) {
            return false;
        }

        let mut dialog_result = None;
        let mut should_close = false;

        if let Some(dialog) = self.window_state.dialog_manager.get_dialog_mut(window_id) {
            dialog.handle_event(event);

            // 检查对话框是否应该关闭
            should_close = dialog.should_close();
            tracing::debug!("对话框 should_close: {}", should_close);

            // 检查对话框结果（handle_event 只入队事件，不会处理消息）
            if let Some(result) = dialog.check_result() {
                tracing::debug!(
                    "对话框结果（第一次检查）: {:?}",
                    std::mem::discriminant(&result)
                );
                dialog_result = Some(result);
                should_close = true;
            }

            // 请求重绘并处理待处理事件（process_pending_events）
            // 注意：事件处理在 redraw 中同步执行，可能产生 dialog_result
            if !should_close {
                tracing::debug!("调用 dialog.redraw()");
                dialog.redraw();
            } else {
                tracing::debug!("跳过 dialog.redraw()，因为 should_close = true");
            }

            // 再次检查结果：redraw 可能处理了事件并设置了 dialog_result
            if dialog_result.is_none() {
                should_close = dialog.should_close();
                if let Some(result) = dialog.check_result() {
                    tracing::debug!(
                        "对话框结果（第二次检查）: {:?}",
                        std::mem::discriminant(&result)
                    );
                    dialog_result = Some(result);
                    should_close = true;
                }
            }
        }

        // 如果应该关闭，关闭对话框
        if should_close {
            self.window_state.dialog_manager.close_dialog(window_id);
        }

        // 处理对话框返回的结果
        if let Some(result) = dialog_result {
            tracing::debug!("处理对话框结果: {:?}", std::mem::discriminant(&result));
            self.process_dialog_result(result);
        }

        true
    }

    /// 处理对话框结果
    fn process_dialog_result(&mut self, result: DialogResult) {
        match &result {
            DialogResult::LoadConfirm => {
                if let Some(path) = self.file_state.pending_load_path.take() {
                    self.load_midi_file(path);
                } else {
                    tracing::warn!("LoadConfirm: 没有 pending 的加载路径");
                }
            }
            DialogResult::Cancel => {
                tracing::info!("对话框: 取消");
            }
            DialogResult::AudioExport {
                project_name,
                midi_path,
                soundfont_path,
                output_path,
                sample_rate,
                channels,
                layers,
                channel_threading,
                key_threading,
                apply_limiter,
                disable_fade_out,
                linear_envelope,
                interpolation,
                format,
            } => {
                tracing::info!(
                    "开始音频导出: project={}, midi={}, sf2={}, output={}",
                    project_name,
                    midi_path,
                    soundfont_path,
                    output_path,
                );

                // 验证 MIDI 文件和音色库是否存在
                let midi = std::path::PathBuf::from(midi_path);
                let sf2 = std::path::PathBuf::from(soundfont_path);
                let output = std::path::PathBuf::from(output_path.clone());

                if !midi.exists() {
                    tracing::error!("MIDI 文件不存在: {:?}", midi);
                    return;
                }
                if !sf2.exists() {
                    tracing::error!("音色库文件不存在: {:?}", sf2);
                    return;
                }

                // 构造导出选项
                let options = lumino_export::audio::AudioExportOptions {
                    sample_rate: *sample_rate,
                    channels: match channels {
                        UiAudioChannels::Mono => lumino_export::audio::AudioChannels::Mono,
                        UiAudioChannels::Stereo => lumino_export::audio::AudioChannels::Stereo,
                    },
                    layers: *layers,
                    channel_threading: match channel_threading {
                        UiThreadingOption::None => lumino_export::audio::ThreadingOption::None,
                        UiThreadingOption::Auto => lumino_export::audio::ThreadingOption::Auto,
                        UiThreadingOption::Manual(n) => {
                            lumino_export::audio::ThreadingOption::Manual(*n)
                        }
                    },
                    key_threading: match key_threading {
                        UiThreadingOption::None => lumino_export::audio::ThreadingOption::None,
                        UiThreadingOption::Auto => lumino_export::audio::ThreadingOption::Auto,
                        UiThreadingOption::Manual(n) => {
                            lumino_export::audio::ThreadingOption::Manual(*n)
                        }
                    },
                    apply_limiter: *apply_limiter,
                    disable_fade_out: *disable_fade_out,
                    linear_envelope: *linear_envelope,
                    interpolation: match interpolation {
                        UiInterpolation::None => lumino_export::audio::Interpolation::None,
                        UiInterpolation::Linear => lumino_export::audio::Interpolation::Linear,
                    },
                    format: match format {
                        UiAudioFormat::WAV => lumino_export::audio::AudioFormat::WAV,
                        UiAudioFormat::FLAC => lumino_export::audio::AudioFormat::FLAC,
                    },
                };

                let cb = self.window_state.progress_cb.clone();
                let output_display = output_path.clone();

                // 在后台线程中执行导出
                tokio::spawn(async move {
                    cb("正在导出音频...", 0.0);
                    let midi = midi.clone();
                    let sf2 = sf2.clone();
                    let output = output.clone();
                    let options_clone = options.clone();
                    let cb_inner = cb.clone();

                    let result = tokio::task::spawn_blocking(move || {
                        lumino_export::audio::export_audio(
                            &midi,
                            &sf2,
                            &output,
                            &options_clone,
                            Some(Arc::new(move |p| {
                                cb_inner(&format!("导出中... {:.0}%", p as f64), p as f64 / 100.0);
                            })),
                            None,
                        )
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
            DialogResult::ProjectSettings {
                title,
                tempo,
                copyright,
            } => {
                tracing::info!(
                    "应用工程设置(对话框结果): 标题={}, BPM={}, 版权={}",
                    title,
                    tempo,
                    copyright
                );
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_project_settings(title.clone(), *tempo, copyright.clone());
                // 更新主窗口标题
                self.window_state
                    .window
                    .window()
                    .set_title(&format!("{} - Lumino", title));
            }
            DialogResult::Settings { settings, theme } => {
                tracing::info!("应用设置面板配置到主窗口，主题: {}", theme);
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_settings(settings.clone(), theme.clone());
            }
            _ => {
                let main_ui = self.window_state.window.ui_mut();
                crate::runner::inner::RunnerInner::apply_dialog_result_to_ui(main_ui, result);
            }
        }
    }
}
