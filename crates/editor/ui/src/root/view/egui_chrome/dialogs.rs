//! 对话框整窗模型：生产 `SettingsPanel` / 主题 → egui 的 `DialogModel`。
//!
//! 单独成文件把 `egui_chrome.rs` 保持在 400 行以内；渲染在
//! `lumino_ui_egui::dialog` / `settings`，这里只做数据映射。

use lumino_core::storage::config::{EraserBehavior, TrackAddBehavior};
use lumino_core::tr;
use lumino_ui_egui::chrome::{
    DialogModel, DialogPalette, SaveConfirmModel, SettingsLabels, SettingsPalette, SettingsSnapshot,
};
use lumino_ui_egui::layout;

use crate::root::Root;
use crate::state::root_state::DialogType;

use super::color32;

/// 生产设置面板文案 → egui 设置对话框模型（中/英文同一条链路）。
fn settings_labels(language: lumino_extras::i18n::Language) -> SettingsLabels {
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

impl Root {
    /// 生产 iced 调色板 → 设置对话框调色板。
    ///
    /// 深色下 `border` / `text_dim` 没有一一对应的 iced 角色，沿用与既有像素
    /// 基线一致的常量；浅色下取 `background.strong` 作为分隔线/次级文字。
    fn settings_palette(&self) -> SettingsPalette {
        let palette = self.window.theme.extended_palette();
        let dark = palette.background.weakest.color.r < 0.5;
        SettingsPalette {
            titlebar: color32(palette.background.neutral.color),
            background: color32(palette.background.base.color),
            panel: color32(palette.background.weakest.color),
            toolbar: color32(palette.background.weak.color),
            card: color32(palette.background.base.color),
            border: if dark {
                SettingsPalette::DARK.border
            } else {
                color32(palette.background.strong.color)
            },
            input_border: color32(palette.background.strong.color),
            text: color32(palette.background.base.text),
            text_dim: if dark {
                SettingsPalette::DARK.text_dim
            } else {
                color32(palette.background.strong.color)
            },
            accent: color32(palette.primary.base.color),
            accent_strong: color32(palette.primary.strong.color),
        }
    }

    /// 生产 iced 调色板 → 对话框调色板（深/浅主题都成立）。
    fn dialog_palette(&self) -> DialogPalette {
        let palette = self.window.theme.extended_palette();
        DialogPalette {
            titlebar: color32(palette.background.neutral.color),
            background: color32(palette.background.base.color),
            text: color32(palette.background.base.text),
            primary: color32(palette.primary.base.color),
            primary_border: color32(palette.primary.weak.color),
            secondary: color32(palette.background.weak.color),
            secondary_border: color32(palette.background.weaker.color),
        }
    }

    /// 已支持整窗接管的对话框（当前仅「未保存更改确认」）。
    ///
    /// 其余对话框（设置 / 导出 / 历史 …）仍由 iced 绘制：整块返回空模型，
    /// 叠加层不画任何东西，避免把主窗口 band 盖到对话框上。
    pub(super) fn egui_dialog_model(&self, viewport: egui::Vec2) -> Option<DialogModel> {
        match self.state.dialog_type {
            DialogType::SaveConfirm => Some(DialogModel::SaveConfirm(SaveConfirmModel {
                titlebar: !self.settings.synth.use_native_titlebar,
                palette: self.dialog_palette(),
                message: tr!("是否保留未保存的更改?").to_string(),
                save: tr!("保存").to_string(),
                close: tr!("关闭").to_string(),
                cancel: tr!("取消").to_string(),
            })),
            DialogType::Settings => {
                // 只接管「常规」页：其余页面的表单尚未建模，整块交回 iced。
                if self.settings.selected_menu_index != 0 {
                    return None;
                }
                // 菜单放不下时交回 iced：叠加层不消费滚轮事件，无法复刻 iced
                // `scrollable` 的滚动位置与滚动条，硬画会把「云管理/兼容性/关于」
                // 三页藏起来（削减 UI）。
                let titlebar_h = if self.settings.synth.use_native_titlebar {
                    0.0
                } else {
                    layout::TITLEBAR_H
                };
                let menu_space = viewport.y
                    - titlebar_h
                    - 2.0 * layout::SETTINGS_MARGIN
                    - layout::SETTINGS_CONFIRM_H
                    - layout::SETTINGS_CONFIRM_GAP
                    - 2.0 * layout::SETTINGS_PADDING;
                let menu_needed = layout::SETTINGS_MENU_ROW_H * layout::SETTINGS_MENU_ITEMS as f32;
                if menu_space < menu_needed {
                    return None;
                }
                let language = self.settings.display.language;
                let editing = &self.settings.editing;
                Some(DialogModel::Settings(Box::new(SettingsSnapshot {
                    palette: self.settings_palette(),
                    labels: settings_labels(language),
                    selected: self.settings.selected_menu_index,
                    eraser: lumino_extras::i18n::eraser_behavior_name(
                        editing.eraser_behavior,
                        language,
                    )
                    .to_owned(),
                    track_add: lumino_extras::i18n::track_add_behavior_name(
                        editing.track_add_behavior,
                        language,
                    )
                    .to_owned(),
                    log_retention: self.settings.logging.log_retention_count.to_string(),
                })))
            }
            _ => None,
        }
    }
}
