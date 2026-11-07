//! 设置「编辑 / 调色板 / 关于」三页的模型构建。
//!
//! 三页都复用 `UiPageModel` 通用行模型与字段动作；只有调色板色块、关于页
//! logo/回声洞使用专用行。

use egui::Color32;
use lumino_core::{tr, trf};
use lumino_extras::i18n::settings_translations;
use lumino_ui_core::state::about_egg::{ABOUT_LOGO_SHAKE_AMPLITUDE, ABOUT_LOGO_SIZE};
use lumino_ui_egui::chrome::{CloudConnModel, CloudPageModel, UiField, UiPageModel, UiRow};

use crate::root::Root;

/// 编辑页。
pub(super) fn editing_page(root: &Root) -> UiPageModel {
    let settings = &root.settings;
    let t = settings_translations(settings.display.language);
    let editing = &settings.editing;
    let mut rows = vec![
        UiRow::Section {
            title: t.editing_history_section.to_owned(),
            hint: String::new(),
        },
        UiRow::Text {
            field: UiField::HistoryTotalLimit,
            label: t.editing_history_total_limit.to_owned(),
            hint: t.editing_history_total_limit_hint.to_owned(),
            value: editing.history_total_limit.to_string(),
            placeholder: "100".to_owned(),
            width: 80.0,
            suffix: String::new(),
        },
        UiRow::Text {
            field: UiField::HistoryEntryLimit,
            label: t.editing_history_entry_limit.to_owned(),
            hint: t.editing_history_entry_limit_hint.to_owned(),
            value: editing.history_entry_limit.to_string(),
            placeholder: "1000".to_owned(),
            width: 80.0,
            suffix: String::new(),
        },
        UiRow::Text {
            field: UiField::MergeWindowMs,
            label: t.editing_merge_window.to_owned(),
            hint: t.editing_merge_window_hint.to_owned(),
            value: editing.merge_window_ms.to_string(),
            placeholder: "300".to_owned(),
            width: 80.0,
            suffix: String::new(),
        },
        UiRow::Section {
            title: t.editing_autosave_section.to_owned(),
            hint: String::new(),
        },
        UiRow::Checkbox {
            field: UiField::AutosaveEnabled,
            label: t.editing_autosave_enabled.to_owned(),
            hint: t.editing_autosave_enabled_hint.to_owned(),
            checked: editing.autosave_enabled,
        },
        UiRow::Text {
            field: UiField::AutosaveInterval,
            label: t.editing_autosave_interval.to_owned(),
            hint: t.editing_autosave_interval_hint.to_owned(),
            value: editing.autosave_interval_mins.clone(),
            placeholder: "5".to_owned(),
            width: 80.0,
            suffix: String::new(),
        },
        UiRow::Section {
            title: t.editing_tempo_max_bpm.to_owned(),
            hint: String::new(),
        },
    ];

    let presets = [
        256.0_f64, 512.0, 1024.0, 2048.0, 4096.0, 8192.0, 16384.0, 32767.0, 65536.0,
    ];
    let mut tempo_options: Vec<String> =
        presets.iter().map(|value| format!("{value:.0}")).collect();
    tempo_options.push(t.editing_tempo_custom_option.to_owned());
    let tempo_selected = presets
        .iter()
        .position(|value| (value - editing.tempo_max_bpm).abs() < f64::EPSILON)
        .unwrap_or(presets.len());
    rows.push(UiRow::Combo {
        field: UiField::TempoMaxBpm,
        label: t.editing_tempo_max_bpm.to_owned(),
        hint: t.editing_tempo_max_bpm_hint.to_owned(),
        options: tempo_options,
        selected: tempo_selected,
        width: 120.0,
    });
    rows.push(UiRow::Section {
        title: t.editing_intercept_section.to_owned(),
        hint: String::new(),
    });
    rows.push(UiRow::Checkbox {
        field: UiField::InterceptNotification,
        label: t.editing_intercept_notification.to_owned(),
        hint: t.editing_intercept_notification_hint.to_owned(),
        checked: editing.intercept_notification_enabled,
    });

    UiPageModel {
        title: t.editing_title.to_owned(),
        rows,
    }
}

/// 调色板页。
pub(super) fn palette_page(root: &Root) -> UiPageModel {
    let settings = &root.settings;
    let t = settings_translations(settings.display.language);
    let palette_name = settings.display.selected_palette.clone();
    let locked = lumino_extras::palette::is_palette_locked();
    let mut rows = Vec::new();
    let picker_label = t.palette_select.to_owned();
    if locked {
        rows.push(UiRow::Static {
            text: format!("{picker_label} {palette_name} (MIDI {})", t.palette_locked),
        });
    } else {
        let options: Vec<String> = settings
            .display
            .available_palettes
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        let selected = options
            .iter()
            .position(|name| *name == palette_name)
            .unwrap_or(0);
        rows.push(UiRow::Combo {
            field: UiField::Palette,
            label: picker_label,
            hint: t.palette_hint.to_owned(),
            options,
            selected,
            width: 300.0,
        });
    }
    rows.push(UiRow::Static {
        text: t.palette_colors_info.to_owned(),
    });

    let manager = &*lumino_extras::palette::PALETTE_MANAGER;
    if let Some(palette) = manager.get(&palette_name) {
        rows.push(UiRow::Swatches {
            colors: palette
                .colors
                .iter()
                .map(|color| {
                    Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3])
                })
                .collect(),
        });
    } else {
        rows.push(UiRow::Static {
            text: t.palette_no_preview.to_owned(),
        });
    }

    UiPageModel {
        title: t.palette_title.to_owned(),
        rows,
    }
}

/// 云管理页。
pub(super) fn cloud_page(root: &Root) -> CloudPageModel {
    let cloud = &root.settings.cloud;
    CloudPageModel {
        title: tr!("云存储管理").to_string(),
        alert: cloud
            .alert
            .as_deref()
            .map(|message| trf!("⚠ 云存储提醒：{msg}", msg = message)),
        add_label: tr!("添加连接").to_string(),
        empty_text: tr!("尚未添加云存储连接。支持 FTP / SFTP / WebDAV，连接信息仅存储在本机。")
            .to_string(),
        rows: cloud
            .connections
            .iter()
            .map(|conn| CloudConnModel {
                id: conn.id.clone(),
                title: format!("{} [{}]", conn.name, conn.protocol),
                address: conn.address.clone(),
                status: if conn.online {
                    tr!("在线").to_string()
                } else {
                    tr!("离线").to_string()
                },
                online: conn.online,
                connect_label: if conn.online {
                    tr!("断开").to_string()
                } else {
                    tr!("连接").to_string()
                },
                manage_label: tr!("管理文件").to_string(),
                delete_label: tr!("删除").to_string(),
            })
            .collect(),
    }
}

/// 关于页（logo / 版本信息 / 回声洞）。
pub(super) fn about_page(root: &Root) -> UiPageModel {
    let settings = &root.settings;
    let t = settings_translations(settings.display.language);
    let egg = &root.state.about_egg;
    let echo = &root.state.echo_cave;
    let mut rows = vec![
        UiRow::AboutLogo {
            dx: egg
                .shake_offset()
                .clamp(-ABOUT_LOGO_SHAKE_AMPLITUDE, ABOUT_LOGO_SHAKE_AMPLITUDE),
            size: ABOUT_LOGO_SIZE,
            visible: egg.is_visible_in_page(),
        },
        UiRow::Section {
            title: t.about_title.to_owned(),
            hint: String::new(),
        },
        UiRow::Section {
            title: t.app_name.to_owned(),
            hint: String::new(),
        },
        UiRow::Static {
            text: t.version.to_owned(),
        },
        UiRow::Static {
            text: t.app_description.to_owned(),
        },
        UiRow::EchoCave {
            text: echo.text().to_owned(),
            opacity: echo.opacity(),
        },
    ];
    if !egg.is_visible_in_page() {
        rows.remove(0);
    }
    UiPageModel {
        title: String::new(),
        rows,
    }
}
