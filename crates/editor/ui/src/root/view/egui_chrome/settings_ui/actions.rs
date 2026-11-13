//! 界面页字段动作 → 生产 `Message`。

use lumino_core::locale::LanguagePreference;
use lumino_core::storage::config::{NoteCountDisplay, SelectionBoxMode};
use lumino_extras::i18n::Language;
use lumino_ui_core::window;
use lumino_ui_egui::chrome::UiField;
use lumino_ui_egui::settings::SettingsAction;

use crate::Message;
use crate::root::Root;
use crate::settings::Event;

pub(crate) fn action_message(root: &Root, action: &SettingsAction) -> Option<Message> {
    let _ = root;
    Some(match action {
        SettingsAction::UiTheme(value) => Message::Window(window::Event::Theme(value.clone())),
        SettingsAction::UiCombo { field, index } => match field {
            UiField::Language => {
                let preference = if *index == 0 {
                    LanguagePreference::System
                } else {
                    LanguagePreference::Fixed(*Language::all().get(index - 1)?)
                };
                Message::Settings(Event::LanguagePreferenceChanged(preference))
            }
            UiField::NoteCountDisplay => {
                let value = if *index == 0 {
                    NoteCountDisplay::ProjectSettings
                } else {
                    NoteCountDisplay::StatusBar
                };
                Message::Settings(Event::NoteCountDisplayChanged(value))
            }
            UiField::SelectionBoxMode => {
                let mode = if *index == 0 {
                    SelectionBoxMode::Direct
                } else {
                    SelectionBoxMode::Spring
                };
                Message::Settings(Event::SelectionBoxModeChanged(mode))
            }
            UiField::TempoMaxBpm => {
                const PRESETS: [f64; 9] = [
                    256.0, 512.0, 1024.0, 2048.0, 4096.0, 8192.0, 16384.0, 32767.0, 65536.0,
                ];
                if let Some(bpm) = PRESETS.get(*index) {
                    Message::Settings(Event::TempoMaxBpmChanged(*bpm))
                } else {
                    Message::Settings(Event::TempoMaxBpmCustomOpen)
                }
            }
            UiField::Palette => {
                let name = (*root.settings.display.available_palettes.get(*index)?).to_owned();
                Message::Settings(Event::PaletteChanged(name))
            }
            _ => return None,
        },
        SettingsAction::UiText { field, value } => match field {
            UiField::ProgramFontName => {
                Message::Settings(Event::ProgramFontNameChanged(value.clone()))
            }
            UiField::ProgramFontPath => {
                Message::Settings(Event::ProgramFontPathChanged(value.clone()))
            }
            UiField::AutoScrollFixed => {
                Message::Settings(Event::AutoScrollFixedPositionChanged(value.clone()))
            }
            UiField::AutoScrollTrigger => {
                Message::Settings(Event::AutoScrollPageTriggerOffsetChanged(value.clone()))
            }
            UiField::AutoScrollReturn => {
                Message::Settings(Event::AutoScrollPageReturnPositionChanged(value.clone()))
            }
            UiField::HistoryTotalLimit => {
                Message::Settings(Event::HistoryTotalLimitChanged(value.clone()))
            }
            UiField::HistoryEntryLimit => {
                Message::Settings(Event::HistoryEntryLimitChanged(value.clone()))
            }
            UiField::MergeWindowMs => Message::Settings(Event::MergeWindowMsChanged(value.clone())),
            UiField::AutosaveInterval => {
                Message::Settings(Event::AutosaveIntervalMinsChanged(value.clone()))
            }
            _ => return None,
        },
        SettingsAction::UiToggle { field, checked } => match field {
            UiField::NativeTitlebar => Message::Settings(Event::NativeTitlebarChanged(*checked)),
            UiField::IconHidpi => Message::Settings(Event::IconHiDPIChanged(*checked)),
            UiField::Enable256Key => Message::Settings(Event::Enable256keyChanged(*checked)),
            UiField::VelocityCurveStyle => {
                Message::Settings(Event::VelocityCurveStyleChanged(*checked))
            }
            UiField::PlaybackKeyColors => {
                Message::Settings(Event::PlaybackKeyColorsEnabledChanged(*checked))
            }
            UiField::AutosaveEnabled => Message::Settings(Event::AutosaveEnabledChanged(*checked)),
            UiField::InterceptNotification => {
                Message::Settings(Event::InterceptNotificationChanged(*checked))
            }
            _ => return None,
        },
        SettingsAction::UiSlider { field, value } => match field {
            UiField::AutomationLineThickness => {
                Message::Settings(Event::AutomationLineThicknessChanged(*value))
            }
            UiField::MonitorRefreshInterval => {
                Message::Settings(Event::MonitorRefreshIntervalChanged(*value))
            }
            _ => return None,
        },
        SettingsAction::UiButton { field } if *field == UiField::BrowseProgramFont => {
            Message::Settings(Event::BrowseProgramFont)
        }
        SettingsAction::UiButton { field } if *field == UiField::AboutLogoClicked => {
            Message::Settings(Event::AboutLogoClicked)
        }
        SettingsAction::UiButton { field } if *field == UiField::EchoCaveClicked => {
            Message::Settings(Event::EchoCaveClicked)
        }
        _ => return None,
    })
}
