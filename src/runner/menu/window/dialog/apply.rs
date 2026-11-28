//! 确认 / 应用类对话框事件：把对话框结果落到主窗口，以及导出控制命令。

use std::sync::Arc;

use crate::runner::RunnerInner;
use lumino_message::events::window::dialog::{AudioExportConfig, VideoExportConfig};
use lumino_midi_loader::MidiDocument;

impl RunnerInner {
    pub(super) fn on_apply_custom_precision(&mut self) {
        // 应用精度（在对话框结果中处理）
    }

    pub(super) fn on_apply_project_settings(
        &mut self,
        title: String,
        tempo: f64,
        copyright: String,
        author: String,
        time_signatures: Vec<(u32, u8, u8)>,
        use10: bool,
    ) {
        tracing::info!(
            "应用工程设置: 标题={}, BPM={}, 版权={}, 作者={}, 拍号变化数={}",
            title,
            tempo,
            copyright,
            author,
            time_signatures.len()
        );
        // 应用设置到主窗口
        let main_ui = self.window_state.window.ui_mut();
        main_ui.apply_project_settings(title, tempo, copyright, author, time_signatures, use10);
        self.sync_shape_drawer_use10(use10);
    }

    pub(super) fn on_confirm_speed_change(&mut self, factor: f32) {
        tracing::info!("应用音符变速: 倍率={}", factor);
        // 应用变速到主窗口
        let main_ui = self.window_state.window.ui_mut();
        main_ui.apply_speed_change(factor);
    }

    pub(super) fn on_confirm_batch_edit(&mut self) {
        // 批量编辑通过对话框结果（DialogResult::BatchEdit）处理，
        // 此处不直接应用。
    }

    pub(super) fn on_start_audio_export(
        &mut self,
        config: Box<AudioExportConfig>,
        document: Option<Arc<MidiDocument>>,
    ) {
        self.handle_start_audio_export(config, document);
    }

    pub(super) fn on_start_video_export(
        &mut self,
        config: Box<VideoExportConfig>,
        document: Option<Arc<MidiDocument>>,
    ) {
        self.handle_start_video_export(config, document);
    }

    pub(super) fn on_pause_audio_export(&mut self) {
        if let Some(ctrl) = &self.window_state.audio_export_control {
            ctrl.pause();
            tracing::info!("音频导出已暂停");
        }
    }

    pub(super) fn on_resume_audio_export(&mut self) {
        if let Some(ctrl) = &self.window_state.audio_export_control {
            ctrl.resume();
            tracing::info!("音频导出已继续");
        }
    }
}
