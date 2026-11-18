//! 对话框整窗模型的共享构造：调色板与设置页文案。
//!
//! 从 `dialogs.rs` 拆出（文件长度限制）；这些函数只做「生产状态 → egui
//! 只读模型」的搬运，不含渲染逻辑。

use lumino_core::storage::config::{EraserBehavior, TrackAddBehavior};
use lumino_ui_egui::chrome::{DialogPalette, SettingsLabels, SettingsPalette, ShortcutsPageModel};

use crate::root::Root;

/// 生产设置面板文案 → egui 设置对话框模型（中/英文同一条链路）。
pub(super) fn settings_labels(language: lumino_extras::i18n::Language) -> SettingsLabels {
    let t = lumino_extras::i18n::settings_translations(language);
    SettingsLabels {
        menu: vec![
            t.general.to_owned(),
            t.audio.to_owned(),
            t.ui.to_owned(),
            t.shortcuts.to_owned(),
            t.onion_skin.to_owned(),
            t.palette.to_owned(),
            t.editing.to_owned(),
            t.cloud_manage.to_owned(),
            t.compatibility.to_owned(),
            t.about.to_owned(),
        ],
        title: t.general_title.to_owned(),
        eraser_label: t.eraser_behavior.to_owned(),
        eraser_options: vec![
            lumino_extras::i18n::eraser_behavior_name(EraserBehavior::Default, language).to_owned(),
            lumino_extras::i18n::eraser_behavior_name(EraserBehavior::DirectSelect, language)
                .to_owned(),
        ],
        track_add_label: t.track_add_behavior.to_owned(),
        track_add_options: vec![
            lumino_extras::i18n::track_add_behavior_name(TrackAddBehavior::AutoSwitch, language)
                .to_owned(),
            lumino_extras::i18n::track_add_behavior_name(TrackAddBehavior::StayCurrent, language)
                .to_owned(),
        ],
        log_section: t.log_retention_section.to_owned(),
        log_label: t.log_retention_count.to_owned(),
        // 生产 `text_input("10", ...)`：占位符是字面量，不随语言变化。
        log_placeholder: "10".to_owned(),
        confirm: t.confirm.to_owned(),
    }
}

/// 快捷键页（标题 + 占位说明）。
pub(super) fn shortcuts_page(language: lumino_extras::i18n::Language) -> ShortcutsPageModel {
    let t = lumino_extras::i18n::settings_translations(language);
    ShortcutsPageModel {
        title: t.shortcuts_title.to_owned(),
        placeholder: t.shortcuts_placeholder.to_owned(),
    }
}

impl Root {
    /// 设置对话框调色板：直接跟随当前窗口主题的 egui 色板（与 上一代 同源）。
    pub(super) fn settings_palette(&self) -> SettingsPalette {
        let palette = self.egui_palette();
        SettingsPalette {
            titlebar: palette.titlebar_bg,
            background: palette.base_bg,
            panel: palette.panel_bg,
            toolbar: palette.toolbar_bg,
            card: palette.card_bg,
            border: palette.border,
            input_border: palette.selected_bg,
            text: palette.text,
            text_dim: palette.text_dim,
            accent: palette.accent,
            accent_strong: palette.accent_strong,
        }
    }

    /// 保存确认对话框调色板：同样跟随当前窗口主题。
    pub(super) fn dialog_palette(&self) -> DialogPalette {
        let palette = self.egui_palette();
        DialogPalette {
            titlebar: palette.titlebar_bg,
            background: palette.base_bg,
            text: palette.text,
            primary: palette.accent,
            primary_hover: palette.accent_strong,
            primary_border: palette.accent_dark,
            secondary: palette.toolbar_bg,
            secondary_hover: palette.selected_bg,
            secondary_border: palette.route_bar_bg,
        }
    }
}
