//! 对话框类窗口事件处理

use crate::runner::midi_manager::PortSoundfontsUnsupportedNotice;
use crate::runner::{RunnerInner, dialog_manager::DialogType};
use lumino_core::storage::config::SynthBackend;
use lumino_core::tr;

mod apply;
mod close;
mod input;
mod open;

impl RunnerInner {
    /// 打开「端口音色库未生效」提示窗（REND-005）。
    ///
    /// 去重：已打开/初始化中只聚焦，不叠开第二个；后端切换成功后由
    /// `handle_midi_reinit` 自动取消。
    pub(crate) fn open_port_soundfonts_unsupported(
        &mut self,
        notice: PortSoundfontsUnsupportedNotice,
    ) {
        let backend = backend_label(notice.backend);
        if self.focus_dialog_if_open(
            DialogType::PortSoundfontsUnsupported,
            "端口音色库未生效提示已打开或在初始化中，忽略重复请求",
        ) {
            return;
        }
        tracing::info!(
            "端口音色库未生效：当前后端 {backend} 不支持，弹窗告知用户（已配置 {} 个端口）",
            notice.ports
        );
        self.window_state
            .dialog_manager
            .open_port_soundfonts_unsupported(backend, notice.ports);
    }
}

/// 后端在提示文案中的显示名。
fn backend_label(backend: SynthBackend) -> String {
    match backend {
        SynthBackend::XSynth => "XSynth".to_string(),
        SynthBackend::Lgs => "LGS (GPU)".to_string(),
        SynthBackend::Maestro => "Maestro".to_string(),
        SynthBackend::Kdmapi => "KDMAPI".to_string(),
        SynthBackend::System => tr!("系统 MIDI").to_string(),
    }
}

impl RunnerInner {
    /// 按对话框类别分发事件；各分支顺序与公开行为保持不变。
    pub(crate) fn handle_dialog_events(
        &mut self,
        window_event: lumino_ui::event::window::dialog::Event,
    ) {
        use lumino_ui::event::window::dialog::Event::*;
        match window_event {
            OpenCustomPrecisionDialog => self.on_open_custom_precision(),
            OpenBrushSettingsDialog(config) => self.on_open_brush_settings(config),
            OpenDrawSettingsDialog => self.on_open_draw_settings(),
            CloseDrawSettingsDialog => self.on_close_draw_settings(),
            OpenShapeDrawerDialog => self.on_open_shape_drawer(),
            CloseShapeDrawerDialog => self.on_close_shape_drawer(),
            OpenHistoryDialog => self.on_open_history(),
            CloseHistoryDialog => self.on_close_history(),
            ToggleHelpDialog => self.on_toggle_help(),
            CloseHelpDialog => self.on_close_help(),
            HelpTipSeen(id) => self.on_help_tip_seen(id),
            ResetHelpTips => self.on_reset_help_tips(),
            CloseCustomPrecisionDialog => self.on_close_custom_precision(),
            ApplyCustomPrecision(_, _) => self.on_apply_custom_precision(),
            OpenCollaborationDialog => self.on_open_collaboration(),
            CloseCollaborationDialog => self.on_close_collaboration(),
            OpenProjectSettingsDialog => self.on_open_project_settings(),
            CloseProjectSettingsDialog => self.on_close_project_settings(),
            OpenPortSoundfontsDialog => self.on_open_port_soundfonts(),
            ApplyProjectSettings {
                title,
                tempo,
                copyright,
                author,
                time_signatures,
                use10,
            } => self.on_apply_project_settings(
                title,
                tempo,
                copyright,
                author,
                time_signatures,
                use10,
            ),
            OpenSpeedChangeDialog => self.on_open_speed_change(),
            OpenMaterialDeleteDialog { index, name } => self.on_open_material_delete(index, name),
            OpenTempoMaxBpmDialog { current_bpm } => self.on_open_tempo_max_bpm(current_bpm),
            OpenBatchEditDialog => self.on_open_batch_edit(),
            OpenVideoExportDialog => self.on_open_video_export(),
            OpenMemoryMonitorDialog => self.on_open_memory_monitor(),
            CloseMemoryMonitorDialog => self.on_close_memory_monitor(),
            CloseVideoExportDialog => self.on_close_video_export(),
            CloseSpeedChangeDialog => self.on_close_speed_change(),
            ConfirmSpeedChange(factor) => self.on_confirm_speed_change(factor),
            CloseBatchEditDialog => self.on_close_batch_edit(),
            ConfirmBatchEdit { .. } => self.on_confirm_batch_edit(),
            OpenLoadConfirmDialog { .. } => self.on_open_load_confirm(),
            OpenRecoverTrackDialog => self.on_open_recover_track(),
            CloseRecoverTrackDialog => self.on_close_recover_track(),
            StartAudioExport { config, document } => self.on_start_audio_export(config, document),
            StartVideoExport { config, document } => self.on_start_video_export(config, document),
            PauseAudioExport => self.on_pause_audio_export(),
            ResumeAudioExport => self.on_resume_audio_export(),
            AbortAudioExport => self.on_abort_audio_export(),
        }
    }
}
