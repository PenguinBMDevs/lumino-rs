//! 混音台接线单测：主窗口快照、对话框模型与动作映射（不依赖真机点击）。

use lumino_ui_egui::mixer::{MixerAction, MixerHit, MixerLayout};

use crate::root::Root;
use crate::state::root_state::DialogType;

fn root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        DialogType::Mixer,
    )
}

fn open_mixer(root: &mut Root) {
    root.mixer_panel.open = true;
    root.mixer_panel.master_volume = 100;
}

#[test]
fn mixer_snapshot_is_built_from_state() {
    let mut root = root();
    open_mixer(&mut root);
    let model = root.mixer_snapshot().expect("打开时应产出混音台快照");

    assert_eq!(model.strips.len(), root.sidebar.tracks.len());
    assert!(model.strips.iter().all(|strip| !strip.master));
    let master = model.master.as_ref().expect("主控条必须存在");
    assert!(master.master);
    assert!((master.volume - 100.0 / 127.0).abs() < 1e-3);
    assert!((model.scroll_x - 0.0).abs() < f32::EPSILON);
}

#[test]
fn mixer_snapshot_is_none_when_closed() {
    let mut root = root();
    root.mixer_panel.open = false;
    assert!(
        root.mixer_snapshot().is_none(),
        "关闭时不应产出快照（Runner 也不会推给对话框）"
    );
}

#[test]
fn dialog_model_uses_local_scroll_and_titlebar() {
    let mut root = root();
    open_mixer(&mut root);
    let snapshot = root.mixer_snapshot().expect("快照");
    assert!(root.set_mixer_dialog_model(snapshot));
    root.mixer_dialog.scroll_x = 42.0;

    let model = root.mixer_dialog_model();
    assert!(
        (model.mixer.scroll_x - 42.0).abs() < 0.01,
        "滚动来自对话框本地状态"
    );
    assert_eq!(
        model.titlebar, !root.settings.synth.use_native_titlebar,
        "标题栏开关必须与配置同源"
    );
    assert!(!model.mixer.strips.is_empty());
}

#[test]
fn set_mixer_dialog_model_detects_changes_only() {
    let mut root = root();
    open_mixer(&mut root);
    let snapshot = root.mixer_snapshot().expect("快照");
    assert!(
        root.set_mixer_dialog_model(snapshot.clone()),
        "首次注入应有变化"
    );
    assert!(
        !root.set_mixer_dialog_model(snapshot),
        "同一快照重复注入不得置脏"
    );
}

#[test]
fn mixer_actions_map_to_sidebar_events() {
    let mut root = root();
    open_mixer(&mut root);

    let msgs = [
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
    // 本地滚动不产生主窗口消息。
    assert!(
        root.mixer_action_message(&MixerAction::Scrolled(30.0))
            .is_none(),
        "横向滚动是对话框本地状态，不得回放成 SidebarEvent"
    );
}

#[test]
fn mixer_dialog_action_routes_scroll_locally() {
    let mut root = root();
    root.apply_mixer_dialog_action(MixerAction::Scrolled(30.0));
    assert!((root.mixer_dialog.scroll_x - 30.0).abs() < 0.01);
    assert!(
        root.take_mixer_dialog_actions().is_empty(),
        "滚动不得进入待回放队列"
    );

    root.apply_mixer_dialog_action(MixerAction::Mute(0));
    assert_eq!(root.take_mixer_dialog_actions(), vec![MixerAction::Mute(0)]);
    assert!(
        root.take_mixer_dialog_actions().is_empty(),
        "取走后不得残留"
    );
}

#[test]
fn mixer_hit_test_covers_buttons_only() {
    let mut root = root();
    open_mixer(&mut root);
    let model = root.mixer_snapshot().expect("快照");
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(760.0, 240.0));
    let layout = MixerLayout::dialog(rect, false, &model);

    let strip = layout.strip_rect(0, model.scroll_x);
    let fader = egui::pos2(strip.center().x, strip.top() + 60.0);
    assert_eq!(
        lumino_ui_egui::mixer::hit_test(fader, &layout, &model),
        None,
        "推子由标准 egui 滑杆处理，自定义命中必须让路"
    );

    let third = strip.width() / 3.0;
    let mute = egui::pos2(strip.left() + third * 1.5, strip.bottom() - 24.0);
    assert!(matches!(
        lumino_ui_egui::mixer::hit_test(mute, &layout, &model),
        Some(MixerHit::Mute(0))
    ));

    let master = layout.master.expect("主控条");
    assert_eq!(
        lumino_ui_egui::mixer::hit_test(master.center(), &layout, &model),
        None,
        "主控推子同样由标准滑杆处理"
    );
}
