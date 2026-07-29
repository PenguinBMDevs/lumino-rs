//! 对话框类窗口事件处理

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use std::sync::atomic::Ordering;

impl RunnerInner {
    pub(crate) fn handle_dialog_events(
        &mut self,
        window_event: lumino_ui::event::window::dialog::Event,
    ) {
        use lumino_ui::event::window::dialog::Event::*;
        match window_event {
            OpenCustomPrecisionDialog => {
                self.open_dialog_traced(DialogType::CustomPrecision, "自定义精度")
            }
            CloseCustomPrecisionDialog => {
                self.close_dialog_traced(DialogType::CustomPrecision, "自定义精度")
            }
            ApplyCustomPrecision(_, _) => {
                // 应用精度（在对话框结果中处理）
            }
            OpenCollaborationDialog => self.open_dialog_traced(DialogType::Collaboration, "协作"),
            CloseCollaborationDialog => self.close_dialog_traced(DialogType::Collaboration, "协作"),
            OpenProjectSettingsDialog => {
                tracing::info!("请求打开工程设置对话框");
                // 优先使用已保存的项目标题，回退到文件名
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
            CloseProjectSettingsDialog => {
                self.close_dialog_traced(DialogType::ProjectSettings, "工程设置")
            }
            ApplyProjectSettings {
                title,
                tempo,
                copyright,
                time_signatures,
            } => {
                tracing::info!(
                    "应用工程设置: 标题={}, BPM={}, 版权={}, 拍号变化数={}",
                    title,
                    tempo,
                    copyright,
                    time_signatures.len()
                );
                // 应用设置到主窗口
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_project_settings(title, tempo, copyright, time_signatures);
            }
            OpenSpeedChangeDialog => self.open_dialog_traced(DialogType::SpeedChange, "音符变速"),
            OpenBatchEditDialog => self.open_dialog_traced(DialogType::BatchEdit, "批量编辑"),
            OpenVideoExportDialog => self.open_dialog_traced(DialogType::VideoExport, "视频导出"),
            OpenMemoryMonitorDialog => {
                self.open_dialog_traced(DialogType::MemoryMonitor, "内存监控")
            }
            CloseMemoryMonitorDialog => {
                self.close_dialog_traced(DialogType::MemoryMonitor, "内存监控")
            }
            CloseVideoExportDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::VideoExport);
                // 设置取消标志，让后台 video-render 线程退出
                self.window_state
                    .video_export_cancel
                    .store(true, Ordering::Relaxed);
            }
            CloseSpeedChangeDialog => self.close_dialog_traced(DialogType::SpeedChange, "音符变速"),
            ConfirmSpeedChange(factor) => {
                tracing::info!("应用音符变速: 倍率={}", factor);
                // 应用变速到主窗口
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_speed_change(factor);
            }
            CloseBatchEditDialog => self.close_dialog_traced(DialogType::BatchEdit, "批量编辑"),
            ConfirmBatchEdit { .. } => {
                // 批量编辑通过对话框结果（DialogResult::BatchEdit）处理，
                // 此处不直接应用。
            }
            OpenLoadConfirmDialog { .. } => {}
            StartAudioExport { config, document } => {
                self.handle_start_audio_export(config, document);
            }
            StartVideoExport { config, document } => {
                self.handle_start_video_export(config, document);
            }
        }
    }
}
