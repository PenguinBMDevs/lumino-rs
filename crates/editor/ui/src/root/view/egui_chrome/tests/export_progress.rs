//! 音频导出进度对话框：模型镜像（只读）。

use super::*;
use lumino_ui_egui::chrome::{DialogModel, ExportMessageKind};

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::ExportProgress,
    )
}

#[test]
fn export_progress_model_mirrors_state() {
    let mut root = dialog_root();
    root.state.export_progress_dialog.message = "正在渲染第 2 段".to_owned();
    root.state.export_progress_dialog.progress = 0.421;

    let model = root.egui_chrome_model();
    let Some(DialogModel::ExportProgress(panel)) = model.dialog else {
        panic!("导出进度应由 egui 整窗接管");
    };
    assert!(panel.titlebar, "默认配置使用自制标题栏");
    assert_eq!(panel.title, "音频导出");
    assert_eq!(panel.message, "正在渲染第 2 段");
    assert_eq!(panel.kind, ExportMessageKind::Normal);
    assert!((panel.progress - 0.421).abs() < 1e-6);
    assert_eq!(panel.percentage, "42.1%");

    // 完成态：文案换成「导出完成」+ 成功色。
    root.state.export_progress_dialog.is_completed = true;
    let model = root.egui_chrome_model();
    let Some(DialogModel::ExportProgress(panel)) = model.dialog else {
        panic!("导出进度应由 egui 整窗接管");
    };
    assert_eq!(panel.message, "导出完成");
    assert_eq!(panel.kind, ExportMessageKind::Completed);

    // 错误优先于完成：错误信息 + 危险色。
    root.state.export_progress_dialog.error = Some("导出失败：磁盘已满".to_owned());
    let model = root.egui_chrome_model();
    let Some(DialogModel::ExportProgress(panel)) = model.dialog else {
        panic!("导出进度应由 egui 整窗接管");
    };
    assert_eq!(panel.message, "导出失败：磁盘已满");
    assert_eq!(panel.kind, ExportMessageKind::Error);
}
