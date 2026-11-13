//! 音频导出面板：模型镜像（选项表 / GPU 禁用 / 警告行 / 三态按钮）与动作映射。

use super::*;
use lumino_message::{AudioBackend, AudioChannels, AudioFormat, Interpolation, ThreadingOption};
use lumino_ui_egui::audio_export::AudioExportAction as EguiAction;
use lumino_ui_egui::chrome::{AudioButtonsMode, AudioToggleId, ExportMessageKind};

fn root_with(state: crate::state::root_state::AudioExportDialogState) -> Root {
    let mut root = Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::None,
    );
    root.state.audio_export_dialog = state;
    root
}

#[test]
fn model_mirrors_state_and_choices() {
    let mut state = crate::state::root_state::AudioExportDialogState::new();
    state.project_name = "我的工程".to_owned();
    state.midi_path = "C:/a.mid".to_owned();
    state.output_path = "C:/out.flac".to_owned();
    state.format = AudioFormat::FLAC;
    state.backend = AudioBackend::Cpu;
    state.sample_rate = 96000;
    state.channels = AudioChannels::Mono;
    state.channel_threading = ThreadingOption::Manual(4);
    state.key_threading = ThreadingOption::None;
    state.interpolation = Interpolation::None;
    state.layers = 0;
    let root = root_with(state);

    let model = root.audio_export_model();
    assert_eq!(model.title, "音频导出");
    assert_eq!(model.project.name, "我的工程");
    assert_eq!(model.project.midi_path, "C:/a.mid");
    assert_eq!(model.output.path, "C:/out.flac");

    // 选项表：选中项与标签。
    assert_eq!(model.settings.formats.len(), 5);
    assert!(model.settings.formats[1].selected, "FLAC 应高亮");
    assert_eq!(model.settings.formats[1].label, "FLAC");
    assert_eq!(model.settings.backends.len(), 2);
    assert!(model.settings.backends[0].selected);
    assert_eq!(model.settings.sample_rates.len(), 4);
    assert!(model.settings.sample_rates[3].selected, "96000 应高亮");
    assert_eq!(model.settings.channels.len(), 2);
    assert!(model.settings.channels[0].selected, "单声道应高亮");
    assert_eq!(model.settings.channel_threads.len(), 5);
    assert!(model.settings.channel_threads[3].selected, "4 线程应高亮");
    assert_eq!(model.settings.key_threads[0].label, "关闭");
    assert!(model.settings.interpolations[0].selected, "无插值应高亮");

    // layers=0 → 危险色警告行。
    assert!(model.settings.layers_warning.is_some());
    assert_eq!(model.settings.layers, "0");

    // 未渲染 → 关闭 / 导出。
    assert!(matches!(
        model.buttons.mode,
        AudioButtonsMode::Actions { .. }
    ));
}

#[test]
fn gpu_disables_limiter_and_fade_toggles() {
    let mut state = crate::state::root_state::AudioExportDialogState::new();
    state.backend = AudioBackend::Gpu;
    let root = root_with(state);
    let model = root.audio_export_model();
    let toggles = &model.settings.toggles;
    assert_eq!(toggles.len(), 3);
    assert_eq!(toggles[0].id, AudioToggleId::ApplyLimiter);
    assert!(!toggles[0].enabled, "GPU 下限幅固定，禁用");
    assert!(toggles[0].checked, "GPU 始终限幅");
    assert!(!toggles[1].enabled, "GPU 下淡出固定，禁用");
    assert!(!toggles[1].checked);
    assert!(toggles[2].enabled, "线性包络仍可切换");
}

#[test]
fn buttons_follow_render_state() {
    let mut state = crate::state::root_state::AudioExportDialogState::new();
    state.is_rendering = true;
    state.render_progress = 0.5;
    state.render_message = "正在渲染".to_owned();
    let root = root_with(state);
    let model = root.audio_export_model();
    match model.buttons.mode {
        AudioButtonsMode::Rendering {
            percentage,
            pause,
            abort,
            ..
        } => {
            assert_eq!(percentage, "50.0%");
            assert_eq!(pause, "暂停");
            assert_eq!(abort, "中止");
        }
        other => panic!("应为渲染中三态，实际 {other:?}"),
    }

    let mut state = crate::state::root_state::AudioExportDialogState::new();
    state.render_completed = true;
    let root = root_with(state);
    match root.audio_export_model().buttons.mode {
        AudioButtonsMode::Finished { status, kind, .. } => {
            assert_eq!(status, "导出完成");
            assert_eq!(kind, ExportMessageKind::Completed);
        }
        other => panic!("应为完成三态，实际 {other:?}"),
    }

    let mut state = crate::state::root_state::AudioExportDialogState::new();
    state.render_error = Some("磁盘已满".to_owned());
    let root = root_with(state);
    match root.audio_export_model().buttons.mode {
        AudioButtonsMode::Finished { status, kind, .. } => {
            assert!(status.contains("磁盘已满"));
            assert_eq!(kind, ExportMessageKind::Error);
        }
        other => panic!("应为失败三态，实际 {other:?}"),
    }
}

#[test]
fn actions_map_to_audio_export_messages() {
    let root = root_with(crate::state::root_state::AudioExportDialogState::new());
    use crate::message::AudioExportAction as Prod;
    type Check = fn(&crate::Message) -> bool;
    let cases: Vec<(EguiAction, Check)> = vec![
        (EguiAction::BackendChanged(1), |m| {
            matches!(
                m,
                crate::Message::AudioExport(Prod::BackendChanged(AudioBackend::Gpu))
            )
        }),
        (EguiAction::FormatChanged(2), |m| {
            matches!(
                m,
                crate::Message::AudioExport(Prod::FormatChanged(AudioFormat::MP3))
            )
        }),
        (EguiAction::SampleRateChanged(0), |m| {
            matches!(
                m,
                crate::Message::AudioExport(Prod::SampleRateChanged(22050))
            )
        }),
        (EguiAction::ChannelsChanged(0), |m| {
            matches!(
                m,
                crate::Message::AudioExport(Prod::ChannelsChanged(AudioChannels::Mono))
            )
        }),
        (EguiAction::ChannelThreadingChanged(4), |m| {
            matches!(
                m,
                crate::Message::AudioExport(Prod::ChannelThreadingChanged(
                    ThreadingOption::Manual(8)
                ))
            )
        }),
        (EguiAction::InterpolationChanged(0), |m| {
            matches!(
                m,
                crate::Message::AudioExport(Prod::InterpolationChanged(Interpolation::None))
            )
        }),
        (EguiAction::Toggle(AudioToggleId::FilterKey, true), |m| {
            matches!(m, crate::Message::AudioExport(Prod::FilterKeyChanged(true)))
        }),
        (EguiAction::Confirm, |m| {
            matches!(m, crate::Message::AudioExport(Prod::Confirm))
        }),
        (EguiAction::ClosePanel, |m| {
            matches!(m, crate::Message::AudioExport(Prod::ClosePanel))
        }),
        (EguiAction::TogglePause, |m| {
            matches!(m, crate::Message::AudioExport(Prod::TogglePause))
        }),
        (EguiAction::Abort, |m| {
            matches!(m, crate::Message::AudioExport(Prod::Abort))
        }),
        (EguiAction::ResetRendering, |m| {
            matches!(m, crate::Message::AudioExport(Prod::ResetRendering))
        }),
    ];
    for (action, check) in cases {
        let message = root
            .audio_export_action_message(&action)
            .unwrap_or_else(|| panic!("动作 {action:?} 应映射到消息"));
        assert!(check(&message), "动作 {action:?} 映射错误：{message:?}");
    }
    // 越界下标不得产生消息。
    assert!(
        root.audio_export_action_message(&EguiAction::BackendChanged(9))
            .is_none()
    );
}
