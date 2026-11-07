//! 对话框整窗模型：生产 `SettingsPanel` / 主题 → egui 的 `DialogModel`。
//!
//! 单独成文件把 `egui_chrome.rs` 保持在 400 行以内；渲染在
//! `lumino_ui_egui::dialog` / `settings`，这里只做数据映射。

use lumino_core::storage::config::{EraserBehavior, TrackAddBehavior};
use lumino_core::tr;
use lumino_ui_core::state::{GpuCheckOutcome, GpuCheckRetryGate, GpuCheckUiState};
use lumino_ui_egui::chrome::{
    CompatLine, CompatibilityPageModel, DialogModel, DialogPalette, OnionSkinPageModel,
    SaveConfirmModel, SettingsLabels, SettingsPageModel, SettingsPalette, SettingsSnapshot,
    ShortcutsPageModel,
};

use crate::Message;
use crate::message::SettingsDialogAction;
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

    /// 已支持整窗接管的对话框（保存确认 / 设置「常规」页）。
    ///
    /// 其余对话框（导出 / 历史 …）与其余设置页仍由 iced 绘制：整块返回空模型，
    /// 叠加层不画任何东西，避免把主窗口 band 盖到对话框上。
    pub(super) fn egui_dialog_model(&self) -> Option<DialogModel> {
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
                let language = self.settings.display.language;
                let selected = self.settings.selected_menu_index;
                // 已迁移页面：常规 / 快捷键 / 洋葱皮 / 兼容性；其余页面整块交回 iced。
                let page = match selected {
                    0 => SettingsPageModel::General,
                    1 => SettingsPageModel::Audio(super::settings_audio::page(self)),
                    3 => SettingsPageModel::Shortcuts(shortcuts_page(language)),
                    4 => SettingsPageModel::OnionSkin(self.onion_skin_page()),
                    8 => SettingsPageModel::Compatibility(self.compatibility_page(language)),
                    _ => return None,
                };
                let editing = &self.settings.editing;
                Some(DialogModel::Settings(Box::new(SettingsSnapshot {
                    palette: self.settings_palette(),
                    labels: settings_labels(language),
                    selected,
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
                    page,
                })))
            }
            _ => None,
        }
    }

    /// 洋葱皮页：后端下拉（加载 MIDI 后只读）+ 能力边界提示。
    fn onion_skin_page(&self) -> OnionSkinPageModel {
        use lumino_core::storage::config::OnionSkinBackend;
        OnionSkinPageModel {
            title: tr!("洋葱皮设置").to_string(),
            label: tr!("洋葱皮渲染方式").to_string(),
            options: vec![
                OnionSkinBackend::Gpu.to_string(),
                OnionSkinBackend::Cpu.to_string(),
            ],
            selected: usize::from(self.settings.onion_skin.backend == OnionSkinBackend::Cpu),
            locked: self.settings.onion_skin.backend_locked,
            locked_value: self.settings.onion_skin.backend.to_string(),
            locked_hint: tr!("加载 MIDI 后不可切换，请先关闭 MIDI 工程").to_string(),
            hint: tr!("CPU 模式会禁用纵向卷帘、工程走带、瀑布流与视频导出").to_string(),
        }
    }

    /// 兼容性页：按钮门控 / 结果行 / 三个开关（文案与 iced 页同源）。
    fn compatibility_page(
        &self,
        language: lumino_extras::i18n::Language,
    ) -> CompatibilityPageModel {
        let t = lumino_extras::i18n::settings_translations(language);
        let compat = &self.settings.compat;
        let running = matches!(compat.check_state, GpuCheckUiState::Running);
        let can_check = !running && compat.retry_gate == GpuCheckRetryGate::Ready;

        let mut result_lines = Vec::new();
        match &compat.check_state {
            GpuCheckUiState::Idle => result_lines.push(CompatLine {
                text: t.compat_never_checked.to_owned(),
                dim: true,
            }),
            GpuCheckUiState::Running => result_lines.push(CompatLine {
                text: t.compat_checking.to_owned(),
                dim: true,
            }),
            GpuCheckUiState::Done(result) => {
                let headline = match result.outcome {
                    GpuCheckOutcome::Passed => t.compat_passed,
                    GpuCheckOutcome::TimedOut => t.compat_timeout,
                    _ => t.compat_failed,
                };
                result_lines.push(CompatLine {
                    text: headline.to_owned(),
                    dim: false,
                });
                if result.outcome == GpuCheckOutcome::TimedOut {
                    result_lines.push(CompatLine {
                        text: t.compat_timeout_hint.to_owned(),
                        dim: true,
                    });
                }
                result_lines.push(CompatLine {
                    text: result.detail.clone(),
                    dim: true,
                });
            }
        }
        match compat.retry_gate {
            GpuCheckRetryGate::Ready => {}
            GpuCheckRetryGate::Cooling => result_lines.push(CompatLine {
                text: t.compat_retry_cooling.to_owned(),
                dim: true,
            }),
            GpuCheckRetryGate::Locked => result_lines.push(CompatLine {
                text: t.compat_retry_locked.to_owned(),
                dim: true,
            }),
        }

        CompatibilityPageModel {
            title: t.compatibility_title.to_owned(),
            check_label: if running {
                t.compat_checking
            } else {
                t.compat_check_button
            }
            .to_owned(),
            check_enabled: can_check,
            copy_label: if compat.copied {
                t.compat_copied
            } else {
                t.compat_copy_diagnostics
            }
            .to_owned(),
            copy_enabled: matches!(compat.check_state, GpuCheckUiState::Done(_)),
            result_lines,
            check_on_startup: compat.check_on_startup,
            check_on_startup_label: t.compat_check_on_startup.to_owned(),
            check_on_startup_hint: t.compat_check_on_startup_hint.to_owned(),
            show_warning: !compat.warning_suppressed,
            warning_label: t.compat_show_warning.to_owned(),
            warning_hint: t.compat_show_warning_hint.to_owned(),
            domino: compat.domino_clipboard_enabled,
            domino_enabled: cfg!(windows),
            domino_label: t.compat_domino_clipboard.to_owned(),
            domino_hint: if cfg!(windows) {
                t.compat_domino_clipboard_hint
            } else {
                t.compat_domino_clipboard_unsupported_hint
            }
            .to_owned(),
        }
    }

    /// 设置动作 → 生产 `Message`。
    pub(crate) fn settings_action_message(
        &self,
        action: &lumino_ui_egui::settings::SettingsAction,
    ) -> Option<Message> {
        use crate::settings::Event;
        use lumino_ui_egui::settings::SettingsAction as A;
        Some(match action {
            A::MenuSelected(index) => Message::Settings(Event::MenuSelected(*index)),
            A::EraserBehavior(behavior) => {
                Message::Settings(Event::EraserBehaviorChanged(*behavior))
            }
            A::TrackAddBehavior(behavior) => {
                Message::Settings(Event::TrackAddBehaviorChanged(*behavior))
            }
            A::LogRetention(value) => {
                Message::Settings(Event::LogRetentionCountChanged(value.clone()))
            }
            A::OnionSkinBackend(backend) => {
                Message::Settings(Event::OnionSkinBackendChanged(*backend))
            }
            A::GpuCheckOnStartup(enabled) => {
                Message::Settings(Event::GpuCheckOnStartupChanged(*enabled))
            }
            A::GpuWarningSuppressed(suppressed) => {
                Message::Settings(Event::GpuWarningSuppressedChanged(*suppressed))
            }
            A::DominoClipboard(enabled) => {
                Message::Settings(Event::DominoClipboardEnabledChanged(*enabled))
            }
            A::RunGpuCheck => Message::Settings(Event::RunGpuCompatibilityCheck),
            A::CopyGpuDiagnostics => Message::Settings(Event::CopyGpuDiagnostics),
            A::AudioCombo { .. }
            | A::AudioSlider { .. }
            | A::AudioText { .. }
            | A::AudioToggle { .. }
            | A::AudioButton { .. } => {
                return super::settings_audio::action_message(self, action);
            }
            A::Confirm => Message::SettingsDialog(SettingsDialogAction::CloseDialog),
        })
    }
}

/// 快捷键页（标题 + 占位说明）。
fn shortcuts_page(language: lumino_extras::i18n::Language) -> ShortcutsPageModel {
    let t = lumino_extras::i18n::settings_translations(language);
    ShortcutsPageModel {
        title: t.shortcuts_title.to_owned(),
        placeholder: t.shortcuts_placeholder.to_owned(),
    }
}
