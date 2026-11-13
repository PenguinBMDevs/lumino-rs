//! egui 点击语义回归：宿主把 winit 事件缓冲到一帧再喂给 egui，
//! 因此「按下与抬起落在同一帧」必须仍然算一次点击。
//!
//! 真机现象：点击偶发无响应、需要连点多次。若这条语义不成立，
//! 任何一次跨事件批次的点击都会被 egui 静默丢弃。

use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

fn input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0))),
        events,
        ..Default::default()
    }
}

fn click_at(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

/// 单帧内按下 + 抬起：egui 必须识别为点击。
#[test]
fn click_with_press_and_release_in_one_frame_registers() {
    let ctx = Context::default();
    let pos = pos2(40.0, 20.0);
    let mut clicked = false;

    for frame in 0..2 {
        let events = if frame == 0 {
            vec![Event::PointerMoved(pos)]
        } else {
            vec![
                Event::PointerMoved(pos),
                click_at(pos, true),
                click_at(pos, false),
            ]
        };
        let mut output = ctx.run_ui(input(events), |ui| {
            let response = ui.put(
                Rect::from_min_size(pos2(20.0, 8.0), vec2(60.0, 24.0)),
                egui::Button::new("hit"),
            );
            clicked |= response.clicked();
        });
        output.textures_delta.clear();
    }

    assert!(clicked, "同一帧内的按下+抬起必须算点击");
}

/// 跨帧按下 / 抬起：常规路径。
#[test]
fn click_with_press_and_release_across_frames_registers() {
    let ctx = Context::default();
    let pos = pos2(40.0, 20.0);
    let mut clicked = false;

    for frame in 0..3 {
        let events = match frame {
            0 => vec![Event::PointerMoved(pos)],
            1 => vec![Event::PointerMoved(pos), click_at(pos, true)],
            _ => vec![click_at(pos, false)],
        };
        let mut output = ctx.run_ui(input(events), |ui| {
            let response = ui.put(
                Rect::from_min_size(pos2(20.0, 8.0), vec2(60.0, 24.0)),
                egui::Button::new("hit"),
            );
            clicked |= response.clicked();
        });
        output.textures_delta.clear();
    }

    assert!(clicked, "跨帧的按下/抬起必须算点击");
}

/// 一帧内两次完整点击：egui 的 `clicked()` 是 bool，只算一次。
///
/// 这是帧式 UI 的固有限制：点击快于帧间隔时，同一帧内的多次点击会被合并。
/// 宿主的对策是**不制造无上限渲染自旋**，让帧间隔足够小（事件驱动，空闲不刷帧）。
#[test]
fn two_click_pairs_in_one_frame_fire_one_widget_click() {
    let ctx = Context::default();
    let pos = pos2(40.0, 20.0);
    let mut clicks = 0;

    for frame in 0..2 {
        let events = if frame == 0 {
            vec![Event::PointerMoved(pos)]
        } else {
            vec![
                Event::PointerMoved(pos),
                click_at(pos, true),
                click_at(pos, false),
                click_at(pos, true),
                click_at(pos, false),
            ]
        };
        let mut output = ctx.run_ui(input(events), |ui| {
            let response = ui.put(
                Rect::from_min_size(pos2(20.0, 8.0), vec2(60.0, 24.0)),
                egui::Button::new("hit"),
            );
            if response.clicked() {
                clicks += 1;
            }
        });
        output.textures_delta.clear();
    }

    assert_eq!(
        clicks, 1,
        "同一帧内的多次点击只能触发一次控件动作（帧式 UI 固有限制）"
    );
}
