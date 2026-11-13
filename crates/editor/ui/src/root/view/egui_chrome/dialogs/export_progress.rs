//! 音频导出对话框：生产状态 → egui 模型（只读，无动作）。

use lumino_core::tr;
use lumino_ui_egui::chrome::{ExportMessageKind, ExportProgressModel};

impl crate::root::Root {
    /// 导出进度模型：标题 / 状态行（普通 / 错误 / 完成）/ 进度 / 百分比。
    pub(super) fn export_progress_model(&self) -> ExportProgressModel {
        let state = &self.state.export_progress_dialog;
        let (message, kind) = if let Some(error) = &state.error {
            (error.clone(), ExportMessageKind::Error)
        } else if state.is_completed {
            (tr!("导出完成").to_string(), ExportMessageKind::Completed)
        } else {
            (state.message.clone(), ExportMessageKind::Normal)
        };
        ExportProgressModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("音频导出").to_string(),
            message,
            kind,
            progress: state.progress as f32,
            percentage: lumino_ui_egui::export_progress::percentage_text(state.progress),
        }
    }
}
