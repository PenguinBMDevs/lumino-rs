//! 独立对话框接管层：整窗绘制对话框标题栏与内容。
//!
//! 接管时不绘制任何主窗口 band，只保留「关于」页彩蛋飞行层（它在对话框
//! 内容之上，提前 `return` 前必须画完，否则进入 `Spin` 后整段飞行会消失）。

use crate::icons::IconCache;

use super::model::{ChromeModel, DialogModel, SettingsSnapshot};
use super::overlays::show_egg_layer;
use super::popup_titlebar;

/// 绘制对话框标题栏与内容；返回 `true` 表示本帧由对话框接管。
pub(super) fn show(root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) -> bool {
    // 对话框自制标题栏（30px）：先画，随后各对话框内容让出该区域。
    if let Some(bar) = &model.dialog_titlebar {
        popup_titlebar::show(root, bar, icons);
    }

    let Some(dialog) = &model.dialog else {
        return false;
    };

    show_content(root, dialog, icons);
    // 「关于」页 logo 彩蛋飞行层（UI-007）：设置窗口本身是独立对话框，
    // 必须在 `return` 之前、对话框内容之后绘制（同层后画 = 盖在内容之上）。
    show_egg_layer(root, icons);
    true
}

/// 按 `DialogModel` 分派到各自的整窗渲染器。
fn show_content(root: &mut egui::Ui, dialog: &DialogModel, icons: &mut IconCache) {
    match dialog {
        DialogModel::SaveConfirm(save) => crate::dialog::show_model(root, save, icons),
        // 复用设置场景渲染器：图标缓存由叠加层提供，渲染完再归还。
        DialogModel::Settings(values) => show_settings(root, values, icons),
        DialogModel::ProjectSettings(model) => crate::project_settings::show(root, model),
        DialogModel::PortSoundfonts(model) => crate::port_soundfonts::show(root, model),
        DialogModel::PortSoundfontsUnsupported(model) => {
            crate::port_soundfonts_unsupported::show(root, model)
        }
        DialogModel::History(model) => crate::history::show(root, model, icons),
        DialogModel::BatchEdit(model) => crate::batch_edit::show(root, model),
        DialogModel::SpeedChange(model) => crate::speed_change::show(root, model),
        DialogModel::MaterialDelete(model) => crate::material_delete::show(root, model),
        DialogModel::TempoMaxBpm(model) => crate::tempo_max_bpm::show(root, model),
        DialogModel::CustomPrecision(model) => crate::custom_precision::show(root, model),
        DialogModel::ExportProgress(model) => crate::export_progress::show(root, model),
        DialogModel::Help(model) => crate::help::show(root, model),
        DialogModel::About(model) => crate::about::show(root, model, icons),
        DialogModel::CloudConnect(model) => crate::cloud_connect::show(root, model),
        DialogModel::MemoryMonitor(model) => crate::memory_monitor::show(root, model),
        DialogModel::DrawSettings(model) => crate::draw_settings::show(root, model),
        DialogModel::LoadConfirm(model) => crate::load_confirm::show(root, model),
        DialogModel::BrushSettings(model) => crate::brush_settings::show(root, model),
        DialogModel::RecoverTrack(model) => crate::recover_track::show(root, model),
        DialogModel::Collaboration(model) => crate::collaboration::show(root, model),
        DialogModel::CloudBrowser(model) => crate::cloud_browser::show(root, model),
        DialogModel::CloudNotice(model) => crate::cloud_browser::show_notice(root, model),
        DialogModel::VideoExportOverlay(model) => crate::video_export_overlay::show(root, model),
        DialogModel::AutosaveRecover(model) => crate::autosave_recover::show(root, model),
        DialogModel::Mixer(model) => crate::mixer::show_dialog(root, model),
        DialogModel::Progress(model) => crate::overlay_window::show_progress(root, model),
        DialogModel::DeviceWarning(model) => {
            crate::overlay_window::show_device_warning(root, model)
        }
    }
}

/// 设置对话框：`SettingsState` 持有图标缓存，渲染期间需要独占。
fn show_settings(root: &mut egui::Ui, values: &SettingsSnapshot, icons: &mut IconCache) {
    let mut state = crate::settings::SettingsState {
        icons: std::mem::take(icons),
        selected: values.selected,
        eraser: values.eraser.clone(),
        track_add: values.track_add.clone(),
        log_retention: values.log_retention.clone(),
        labels: values.labels.clone(),
        page: values.page.clone(),
    };
    crate::settings::show_with(root, &mut state, &values.palette);
    *icons = state.icons;
}
