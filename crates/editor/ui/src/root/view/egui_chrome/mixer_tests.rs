//! 混音台接线单测：模型装配与动作映射（不依赖真机点击）。

use lumino_ui_egui::mixer::{MixerAction, MixerHit};

use crate::root::Root;
use crate::state::root_state::DialogType;

fn root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        DialogType::VideoExport,
    )
}

fn open_mixer(root: &mut Root) {
    // 注入探针矩形（真机里来自 iced 面板布局）。
    crate::view::video_clip::rect_probe::set_rect_for_test(
        "mixer::panel",
        iced_core::Rectangle {
            x: 60.0,
            y: 300.0,
            width: 400.0,
            height: 280.0,
        },
    );
    root.mixer_panel.open = true;
    root.mixer_panel.maximized = true;
    root.mixer_panel.master_volume = 100;
}

#[test]
fn mixer_model_is_built_from_state() {
    let mut root = root();
    open_mixer(&mut root);
    let (layout, model) = root.mixer_model().expect("面板打开且矩形已知时应产出模型");

    // 布局贴底：面板底缘 = 探针 y+height。
    assert!((layout.panel.bottom() - 580.0).abs() < 0.01);
    assert!((layout.body.top() - layout.panel.top() - 28.0).abs() < 0.01);

    // 通道条数量 = 音轨数；主控条存在。
    assert_eq!(model.strips.len(), root.sidebar.tracks.len().max(1));
    assert!(model.master.is_some(), "主控条必须存在");
    assert!(model.strips.iter().all(|s| !s.master));
    let master = model.master.as_ref().expect("主控条");
    assert!((master.volume - 100.0 / 127.0).abs() < 1e-3);
}

#[test]
fn mixer_closed_or_unknown_rect_yields_none() {
    let mut root = root();
    root.mixer_panel.open = false;
    assert!(root.mixer_model().is_none(), "关闭时不产出模型");
}

#[test]
fn mixer_actions_map_to_sidebar_events() {
    let mut root = root();
    open_mixer(&mut root);
    let track_id = root.sidebar.tracks.first().map(|t| t.id).unwrap_or(0);

    let msgs = [
        MixerAction::DragStarted,
        MixerAction::DragBy { dx: 4.0, dy: -3.0 },
        MixerAction::DragEnded,
        MixerAction::Mute(0),
        MixerAction::Solo(0),
        MixerAction::MasterVolume(0.5),
        MixerAction::Gain {
            index: 0,
            value: 0.75,
        },
        MixerAction::Pan {
            index: 0,
            value: -0.5,
        },
    ];
    for action in msgs {
        let message = root
            .mixer_action_message(&action)
            .unwrap_or_else(|| panic!("{action:?} 应有对应消息"));
        let debug = format!("{message:?}");
        assert!(!debug.is_empty(), "{action:?} 映射为空");
    }
    // 越界下标不应产出消息（Gain/Pan/Mute/Solo 依赖音轨 ID）。
    let out_of_range = MixerAction::Mute(99);
    assert!(
        root.mixer_action_message(&out_of_range).is_none(),
        "越界下标应返回 None"
    );
    let _ = track_id;
}

#[test]
fn mixer_hit_test_reaches_fader_and_buttons() {
    let mut root = root();
    open_mixer(&mut root);
    let (layout, model) = root.mixer_model().expect("模型");
    // 第一条推子中部。
    let strip = layout.strip_rect(0, model.scroll_x);
    let fader = egui::pos2(strip.center().x, strip.top() + 60.0);
    assert!(matches!(
        lumino_ui_egui::mixer::hit_test(fader, &layout, &model),
        Some(MixerHit::Fader(0))
    ));
    // 标题栏。
    assert!(matches!(
        lumino_ui_egui::mixer::hit_test(layout.title.center(), &layout, &model),
        Some(MixerHit::Title)
    ));
}
