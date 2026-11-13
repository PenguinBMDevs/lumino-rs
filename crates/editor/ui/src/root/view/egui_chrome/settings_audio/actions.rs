//! 音频页字段动作 → 生产 `Message`（从 `settings_audio.rs` 拆出）。

use lumino_core::storage::config::{GpuLoadDisplay, SynthBackend};
use lumino_ui_core::settings_event::OutputType;
use lumino_ui_egui::chrome::AudioField;
use lumino_ui_egui::settings::SettingsAction;

use crate::Message;
use crate::root::Root;
use crate::settings::Event;

/// 字段动作 → 生产 `Message`。
pub(crate) fn action_message(root: &Root, action: &SettingsAction) -> Option<Message> {
    let settings = &root.settings;
    Some(match action {
        SettingsAction::AudioCombo { field, index } => match field {
            AudioField::OutputType => {
                let output = match index {
                    0 => OutputType::Builtin,
                    1 => OutputType::Kdmapi,
                    _ => OutputType::System,
                };
                Message::Settings(Event::OutputTypeChanged(output))
            }
            AudioField::Engine => {
                let backend = if *index == 0 {
                    SynthBackend::XSynth
                } else {
                    SynthBackend::Lgs
                };
                Message::Settings(Event::SynthBackendChanged(backend))
            }
            AudioField::AudioOutput => {
                let name = if *index == 0 {
                    String::new()
                } else {
                    settings
                        .synth
                        .audio_output_devices
                        .get(index - 1)
                        .cloned()
                        .unwrap_or_default()
                };
                Message::Settings(Event::AudioOutputSelected(name))
            }
            AudioField::MidiDevice => {
                let id = settings.midi.devices.get(*index).map(|(id, _)| *id)?;
                Message::Settings(Event::DeviceSelected(id))
            }
            AudioField::WinmmOutput => {
                let id = settings.midi.winmm_outputs.get(*index).map(|(id, _)| *id)?;
                Message::Settings(Event::WinmmOutputSelected(id))
            }
            AudioField::LgsOfflineBlock => {
                let value = [512usize, 1024, 2048, 4096, 8192].get(*index)?;
                Message::Settings(Event::LgsOfflineBlockSizeChanged(*value))
            }
            AudioField::LgsGpuLoadDisplay => {
                let mode = match index {
                    0 => GpuLoadDisplay::ShortAverage,
                    _ => GpuLoadDisplay::Instant,
                };
                Message::Settings(Event::GpuLoadDisplayChanged(mode))
            }
            _ => return None,
        },
        SettingsAction::AudioSlider { field, value } => match field {
            AudioField::XSynthBuffer => {
                Message::Settings(Event::XSynthBufferChanged(f64::from(*value)))
            }
            AudioField::XSynthMaxVoices => {
                let voices = if *value < 0.5 {
                    None
                } else {
                    Some(*value as usize)
                };
                Message::Settings(Event::XSynthMaxVoicesChanged(voices))
            }
            AudioField::XSynthGlobalLimit => {
                Message::Settings(Event::XSynthGlobalVoiceLimitChanged(*value as usize))
            }
            AudioField::XSynthRatio => {
                Message::Settings(Event::XSynthVoiceTargetRatioChanged(f64::from(*value)))
            }
            AudioField::XSynthVelocity => Message::Settings(Event::VelocityFilterThresholdChanged(
                value.round().to_string(),
            )),
            AudioField::LgsBlock => {
                let size = 1usize << (*value as u32);
                Message::Settings(Event::LgsBlockSizeChanged(size))
            }
            AudioField::LgsMaxVoices => {
                Message::Settings(Event::LgsMaxVoicesChanged(*value as usize))
            }
            AudioField::LgsGlobalLimit => {
                Message::Settings(Event::LgsGlobalVoiceLimitChanged(*value as usize))
            }
            AudioField::LgsVelocity => {
                Message::Settings(Event::LgsVelocityFilterChanged(*value as u8))
            }
            _ => return None,
        },
        SettingsAction::AudioText { field, value } => match field {
            AudioField::SoundfontPath => {
                Message::Settings(Event::SoundfontPathChanged(value.clone()))
            }
            AudioField::XSynthMaxVoicesCustom => {
                Message::Settings(Event::XSynthMaxVoicesCustomInput(value.clone()))
            }
            _ => return None,
        },
        SettingsAction::AudioToggle { field, checked } => match field {
            AudioField::XSynthGate => Message::Settings(Event::XSynthSoftNpsGateChanged(*checked)),
            AudioField::LgsGate => Message::Settings(Event::LgsSoftNpsGateChanged(*checked)),
            AudioField::LgsGpuBufferStatus => {
                Message::Settings(Event::GpuBufferStatusEnabledChanged(*checked))
            }
            AudioField::LgsGpuGovernorMetrics => {
                Message::Settings(Event::GpuGovernorMetricsEnabledChanged(*checked))
            }
            _ => return None,
        },
        SettingsAction::AudioButton { field } => match field {
            AudioField::ScanAudioOutputs => Message::Settings(Event::ScanAudioOutputs),
            AudioField::ScanWinmmOutputs => Message::Settings(Event::ScanWinmmOutputs),
            AudioField::BrowseSoundfont => Message::Settings(Event::BrowseSoundfont),
            _ => return None,
        },
        _ => return None,
    })
}
