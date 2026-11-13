//! 剪辑带标题行：传输按钮（倒带 / 播放暂停）的点击必须产出对应动作。
//!
//! 无头 kittest：不依赖 旧版 布局，`title_row` 由模型直接给出（生产里来自探针）。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput, Rect};
use egui_kittest::Harness;
use lumino_ui_egui::video_clip_timeline::{
    ClipEdit, VideoTimelineAction, VideoTimelineModel, show_title_row,
};

fn model(title_row: Option<Rect>, playing: bool) -> VideoTimelineModel {
    VideoTimelineModel {
        duration_secs: 10.0,
        zoom: 1.0,
        scroll_x: 0.0,
        playhead_secs: 0.0,
        video: ClipEdit::default(),
        audio: ClipEdit::default(),
        viewport_w: 400.0,
        title_row,
        scrollbar: None,
        content_w: 0.0,
        title: "剪辑带".to_string(),
        info: "时长 10.0s".to_string(),
        playing,
    }
}

fn harness(model: VideoTimelineModel) -> (Harness<'static>, Rc<RefCell<Vec<VideoTimelineAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 40.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            show_title_row(ui, &model, &mut icons);
            for action in lumino_ui_egui::chrome::take_video_timeline_actions(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

fn click(harness: &mut Harness<'static>, pos: Pos2) {
    let mut input = RawInput::default();
    input.events.push(Event::PointerMoved(pos));
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    *harness.input_mut() = input;
    harness.run();
}

#[test]
fn transport_buttons_emit_rewind_and_play() {
    let row = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 28.0));
    // 从左往右扫标题行：标题后紧跟倒带、播放两个 28px 按钮（间距 4）。
    let mut kinds = Vec::new();
    for x in (20..=150).step_by(6) {
        let (mut h, actions) = harness(model(Some(row), false));
        click(&mut h, Pos2::new(x as f32, 14.0));
        for action in actions.borrow().iter() {
            kinds.push(format!("{action:?}"));
        }
    }
    assert!(
        kinds.iter().any(|k| k.contains("Rewound")),
        "倒带按钮应可点：{kinds:?}"
    );
    assert!(
        kinds.iter().any(|k| k.contains("PlayToggled")),
        "播放按钮应可点：{kinds:?}"
    );
}

#[test]
fn no_title_row_means_no_transport_actions() {
    let (mut h, actions) = harness(model(None, false));
    for x in (20..=240).step_by(20) {
        click(&mut h, Pos2::new(x as f32, 14.0));
    }
    assert!(
        actions.borrow().is_empty(),
        "无标题行矩形时不得产出传输动作"
    );
}

#[test]
fn playing_state_switches_icon_only_not_actions() {
    // 播放中的图标换成暂停，但点击仍产出 `PlayToggled`（切换语义）。
    let row = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 28.0));
    let mut got = Vec::new();
    for playing in [false, true] {
        for x in (20..=150).step_by(6) {
            let (mut h, actions) = harness(model(Some(row), playing));
            click(&mut h, Pos2::new(x as f32, 14.0));
            for action in actions.borrow().iter() {
                got.push(format!("{playing}:{action:?}"));
            }
        }
    }
    assert!(
        got.iter()
            .any(|k| k.starts_with("true:") && k.contains("PlayToggled"))
    );
}
