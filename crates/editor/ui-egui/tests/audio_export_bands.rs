//! 音频导出面板的无头交互测试：控件回传 + 三态按钮。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::audio_export::{self, AudioExportAction};
use lumino_ui_egui::chrome::{
    AudioButtonsMode, AudioButtonsModel, AudioChoice, AudioExportModel, AudioExportPanelModel,
    AudioFilterModel, AudioOutputModel, AudioProjectModel, AudioSettingsModel, AudioToggle,
    AudioToggleId,
};

fn choices(labels: &[&str], selected: usize) -> Vec<AudioChoice> {
    labels
        .iter()
        .enumerate()
        .map(|(index, label)| AudioChoice {
            index,
            label: (*label).to_owned(),
            selected: index == selected,
        })
        .collect()
}

fn toggle(id: AudioToggleId, label: &str, checked: bool, enabled: bool) -> AudioToggle {
    AudioToggle {
        id,
        label: label.to_owned(),
        checked,
        enabled,
    }
}

fn model(mode: AudioButtonsMode) -> AudioExportModel {
    AudioExportModel {
        titlebar: false,
        title: "音频导出".to_owned(),
        project: AudioProjectModel {
            name_label: "工程名称".to_owned(),
            name: "我的工程".to_owned(),
            name_hint: "工程名称".to_owned(),
            midi_label: "MIDI 路径".to_owned(),
            midi_path: "C:/a.mid".to_owned(),
            soundfont_label: "音色库 (SF2)".to_owned(),
            soundfont_path: "C:/a.sf2".to_owned(),
            browse: "浏览...".to_owned(),
        },
        settings: AudioSettingsModel {
            title: "音频设置".to_owned(),
            backend_label: "渲染后端:".to_owned(),
            backends: choices(&["CPU (兼容)", "GPU (加速)"], 0),
            backend_hint: "CPU 兼容性最好".to_owned(),
            format_label: "输出格式:".to_owned(),
            formats: choices(&["WAV", "FLAC", "MP3"], 0),
            format_hint: "仅 MP3/Ogg 有效".to_owned(),
            bitrate_label: "比特率 (kbps):".to_owned(),
            bitrate: "320".to_owned(),
            bitrate_hint: "仅 MP3/Ogg 有效".to_owned(),
            sample_rate_label: "采样率 (Hz):".to_owned(),
            sample_rates: choices(&["44100", "48000"], 1),
            sample_rate_hint: "MP3 最高 48000 Hz".to_owned(),
            channels_label: "通道数:".to_owned(),
            channels: choices(&["单声道", "立体声"], 1),
            layers_label: "每键复音 (0=不限):".to_owned(),
            layers: "32".to_owned(),
            layers_hint: "建议保持 32".to_owned(),
            layers_warning: None,
            channel_thread_label: "通道多线程 (CPU):".to_owned(),
            channel_threads: choices(&["关闭", "自动"], 1),
            key_thread_label: "按键多线程 (CPU):".to_owned(),
            key_threads: choices(&["关闭", "自动"], 1),
            interpolation_label: "插值算法:".to_owned(),
            interpolations: choices(&["无插值", "线性插值"], 1),
            interpolation_hint: "线性为默认".to_owned(),
            toggles: vec![
                toggle(AudioToggleId::ApplyLimiter, "启用限幅器", true, true),
                toggle(AudioToggleId::DisableFadeOut, "禁用淡出", false, true),
                toggle(AudioToggleId::LinearEnvelope, "线性包络", false, true),
            ],
        },
        filter: AudioFilterModel {
            title: "事件过滤".to_owned(),
            ignore: toggle(
                AudioToggleId::IgnoreProgram,
                "忽略音色变化事件",
                false,
                true,
            ),
            ignore_hint: "勾选后丢弃音色切换".to_owned(),
            velocity_title: "音符力度过滤".to_owned(),
            velocity_toggle: toggle(AudioToggleId::FilterVelocity, "启用力度过滤", false, true),
            velocity_range_label: "力度范围 (0-127):".to_owned(),
            velocity_low: "0".to_owned(),
            velocity_high: "127".to_owned(),
            key_title: "音符键位过滤".to_owned(),
            key_toggle: toggle(AudioToggleId::FilterKey, "启用键位过滤", false, true),
            key_range_label: "键位范围 (0-127):".to_owned(),
            key_low: "0".to_owned(),
            key_high: "127".to_owned(),
            delay_label: "音符结束延迟 (ms):".to_owned(),
            delay: "0".to_owned(),
            delay_hint: "0=禁用".to_owned(),
        },
        output: AudioOutputModel {
            title: "输出路径".to_owned(),
            path: "C:/out.wav".to_owned(),
            path_hint: "选择输出路径...".to_owned(),
            browse: "浏览...".to_owned(),
        },
        buttons: AudioButtonsModel { mode },
    }
}

fn actions_mode() -> AudioButtonsMode {
    AudioButtonsMode::Actions {
        close: "关闭".to_owned(),
        export: "导出".to_owned(),
    }
}

fn make_harness(mode: AudioButtonsMode) -> (Harness<'static>, Rc<RefCell<Vec<AudioExportAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let content = model(mode.clone());
    let panel = AudioExportPanelModel {
        rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(760.0, 2400.0)),
        content,
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(760.0, 2400.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            audio_export::show_panel(ui, &panel);
            if let Some(action) = audio_export::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn sections_and_labels_are_visible() {
    let (harness, _) = make_harness(actions_mode());
    for label in [
        "音频导出",
        "工程名称",
        "MIDI 路径",
        "音色库 (SF2)",
        "音频设置",
        "渲染后端:",
        "输出格式:",
        "通道多线程 (CPU):",
        "事件过滤",
        "输出路径",
        "导出",
        "关闭",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}

#[test]
fn export_and_close_dispatch() {
    let (mut harness, actions) = make_harness(actions_mode());
    harness.get_by_label("导出").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&AudioExportAction::Confirm));

    let (mut harness, actions) = make_harness(actions_mode());
    harness.get_by_label("关闭").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&AudioExportAction::ClosePanel)
    );
}

#[test]
fn toggle_dispatches_field_and_value() {
    let (mut harness, actions) = make_harness(actions_mode());
    harness.get_by_label("启用限幅器").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&AudioExportAction::Toggle(
            AudioToggleId::ApplyLimiter,
            false
        ))
    );
}

#[test]
fn bitrate_input_dispatches_text() {
    let (mut harness, actions) = make_harness(actions_mode());
    let input = harness.get_by(|node| {
        node.role() == egui::Role::TextInput && node.value().as_deref() == Some("320")
    });
    input.focus();
    input.type_text("1");
    harness.run();
    assert!(
        actions.borrow().iter().any(|action| matches!(
            action,
            AudioExportAction::BitrateChanged(text) if text.contains('1')
        )),
        "比特率输入应回传 BitrateChanged，实际 {:?}",
        actions.borrow()
    );
}

#[test]
fn rendering_state_shows_progress_and_pause() {
    let (mut harness, actions) = make_harness(AudioButtonsMode::Rendering {
        status: "正在渲染".to_owned(),
        progress: 0.5,
        percentage: "50.0%".to_owned(),
        pause: "暂停".to_owned(),
        abort: "中止".to_owned(),
    });
    assert!(harness.query_by_label("50.0%").is_some());
    harness.get_by_label("暂停").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&AudioExportAction::TogglePause)
    );
    harness.get_by_label("中止").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&AudioExportAction::Abort));
}

#[test]
fn finished_state_shows_reset_and_close() {
    let (mut harness, actions) = make_harness(AudioButtonsMode::Finished {
        status: "导出完成".to_owned(),
        kind: lumino_ui_egui::chrome::ExportMessageKind::Completed,
        reset: "重置".to_owned(),
        close: "关闭".to_owned(),
    });
    assert!(harness.query_by_label("导出完成").is_some());
    harness.get_by_label("重置").click();
    harness.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&AudioExportAction::ResetRendering)
    );
}
