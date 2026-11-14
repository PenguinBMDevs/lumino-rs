//! 走带滚轮回归：触控板水平滑动、Shift+滚轮、斜向双轴与轨道列滚动。
//!
//! 回归背景：
//! 1. 泳道滚轮只处理纵向增量，触控板横向滑动（`delta.x`）被直接丢弃；
//! 2. Shift+滚轮没有像钢琴卷帘那样把纵向增量转成横向，Windows 上
//!    `LineDelta` 的 x 恒为 0，导致 Shift+滚轮完全无效；
//! 3. 一帧内多条滚轮事件（Windows 拆分的横 / 纵两条）只取最后一条，
//!    斜向滑动退化成单轴。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_message::arrangement_action::ArrangementAction as LaneAction;
use lumino_message::types::geometry::Rect2;
use lumino_ui_egui::chrome::ArrangementAction;
use lumino_ui_egui::chrome::arrangement;
use lumino_ui_egui::chrome::{
    ArrangementLaneModel, ArrangementModel, ArrangementRulerModel, ArrangementTrackListModel,
};

const LANE_SCROLL_X: f32 = 100.0;
const LANE_SCROLL_Y: f32 = 50.0;

fn model() -> ArrangementModel {
    ArrangementModel {
        ruler: ArrangementRulerModel {
            scroll_x: LANE_SCROLL_X,
            zoom_x: 0.1,
            ppq: 960,
            left_inset: 0.0,
            top_inset: 0.0,
            time_signatures: vec![(0, 4, 4)],
            total_ticks: 100_000,
        },
        track_list: ArrangementTrackListModel {
            track_height: 48.0,
            scroll_y: LANE_SCROLL_Y,
            ..ArrangementTrackListModel::default()
        },
        lane: Some(ArrangementLaneModel {
            rect: Rect2::new(200.0, 100.0, 500.0, 400.0),
            track_height: 48.0,
            tool: lumino_core::Tool::Pointer,
            selection: None,
            selected_notes: Vec::new(),
            precision: lumino_core::SnapSpec::default(),
        }),
        scrollbars: None,
    }
}

/// 测试夹具：egui harness + 本帧收集到的泳道 / chrome 动作。
struct TestHarness {
    ui: Harness<'static>,
    lane: Rc<RefCell<Vec<LaneAction>>>,
    chrome: Rc<RefCell<Vec<ArrangementAction>>>,
}

fn harness() -> TestHarness {
    let lane_actions = Rc::new(RefCell::new(Vec::new()));
    let lane_actions_w = lane_actions.clone();
    let chrome_actions = Rc::new(RefCell::new(Vec::new()));
    let chrome_actions_w = chrome_actions.clone();
    let mut ui = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            // 轨道列滚动与泳道滚动都走泳道动作通道，这里一并收集；
            // 标尺缩放走 chrome 动作返回值（生产侧会 push_action）。
            if let Some(action) = arrangement::show(ui, ui.max_rect(), &model()) {
                chrome_actions_w.borrow_mut().push(action);
            }
            lane_actions_w
                .borrow_mut()
                .extend(arrangement::take_lane_actions(ui.ctx()));
        });
    ui.run();
    TestHarness {
        ui,
        lane: lane_actions,
        chrome: chrome_actions,
    }
}

fn move_pointer(harness: &mut Harness<'static>, pos: egui::Pos2) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(pos));
    *harness.input_mut() = input;
    harness.run();
}

/// 发送一帧滚轮事件（可同时包含多条，模拟 Windows 拆分后的横 / 纵事件）。
fn wheel(
    harness: &mut Harness<'static>,
    events: &[(egui::MouseWheelUnit, egui::Vec2)],
    modifiers: egui::Modifiers,
) {
    let mut input = egui::RawInput::default();
    if modifiers != egui::Modifiers::default() {
        input.events.push(egui::Event::ModifiersChanged(modifiers));
    }
    for (unit, delta) in events {
        input.events.push(egui::Event::MouseWheel {
            unit: *unit,
            delta: *delta,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers,
        });
    }
    *harness.input_mut() = input;
    harness.run();
}

fn scroll_x(actions: &[LaneAction]) -> Option<f32> {
    actions.iter().rev().find_map(|action| match action {
        LaneAction::ScrollX(x) => Some(*x),
        _ => None,
    })
}

fn scroll_y(actions: &[LaneAction]) -> Option<f32> {
    actions.iter().rev().find_map(|action| match action {
        LaneAction::ScrollY(y) => Some(*y),
        _ => None,
    })
}

/// 触控板水平滑动（像素增量 x < 0，指针在泳道内）→ 时间轴右移。
#[test]
fn trackpad_horizontal_swipe_scrolls_timeline() {
    let mut h = harness();
    move_pointer(&mut h.ui, egui::pos2(400.0, 300.0));

    wheel(
        &mut h.ui,
        &[(egui::MouseWheelUnit::Point, egui::vec2(-24.0, 0.0))],
        egui::Modifiers::default(),
    );

    let x = scroll_x(&h.lane.borrow()).expect("触控板横滑必须产生 ScrollX");
    assert!(
        (x - (LANE_SCROLL_X + 24.0)).abs() < f32::EPSILON,
        "左滑 24px 后 scroll_x 应为 {}，实际 {x}",
        LANE_SCROLL_X + 24.0
    );
    assert!(
        scroll_y(&h.lane.borrow()).is_none(),
        "纯水平滑动不应产生 ScrollY"
    );
}

/// Shift+滚轮（Line，x 恒为 0）→ 纵向增量转横向，与钢琴卷帘一致。
#[test]
fn shift_wheel_down_scrolls_right() {
    let mut h = harness();
    move_pointer(&mut h.ui, egui::pos2(400.0, 300.0));

    wheel(
        &mut h.ui,
        &[(egui::MouseWheelUnit::Line, egui::vec2(0.0, -1.0))],
        egui::Modifiers::SHIFT,
    );

    let x = scroll_x(&h.lane.borrow()).expect("Shift+滚轮向下必须产生 ScrollX");
    assert!(
        (x - (LANE_SCROLL_X + 30.0)).abs() < f32::EPSILON,
        "向下滚 1 行（30px）视图应右移，实际 {x}"
    );
    assert!(
        scroll_y(&h.lane.borrow()).is_none(),
        "Shift+滚轮不应再产生纵向滚动"
    );
}

/// 触控板斜向滑动 → X / Y 双轴都要滚动。
#[test]
fn diagonal_trackpad_wheel_scrolls_both_axes() {
    let mut h = harness();
    move_pointer(&mut h.ui, egui::pos2(400.0, 300.0));

    wheel(
        &mut h.ui,
        &[(egui::MouseWheelUnit::Point, egui::vec2(-24.0, -12.0))],
        egui::Modifiers::default(),
    );

    let x = scroll_x(&h.lane.borrow()).expect("斜向滑动应产生 ScrollX");
    let y = scroll_y(&h.lane.borrow()).expect("斜向滑动应产生 ScrollY");
    assert!(
        (x - (LANE_SCROLL_X + 24.0)).abs() < f32::EPSILON,
        "斜向滑动横向分量错误，实际 {x}"
    );
    assert!(
        (y - (LANE_SCROLL_Y + 12.0)).abs() < f32::EPSILON,
        "斜向滑动纵向分量错误，实际 {y}"
    );
}

/// 同一帧内的多条滚轮事件必须累加，不能后一条覆盖前一条。
#[test]
fn wheel_events_in_one_frame_accumulate() {
    let mut h = harness();
    move_pointer(&mut h.ui, egui::pos2(400.0, 300.0));

    wheel(
        &mut h.ui,
        &[
            (egui::MouseWheelUnit::Line, egui::vec2(0.0, -1.0)),
            (egui::MouseWheelUnit::Line, egui::vec2(-1.0, 0.0)),
        ],
        egui::Modifiers::default(),
    );

    let x = scroll_x(&h.lane.borrow()).expect("横向事件不能被纵向事件覆盖");
    let y = scroll_y(&h.lane.borrow()).expect("纵向事件不能被横向事件覆盖");
    assert!(
        (x - (LANE_SCROLL_X + 30.0)).abs() < f32::EPSILON,
        "实际 x={x}"
    );
    assert!(
        (y - (LANE_SCROLL_Y + 30.0)).abs() < f32::EPSILON,
        "实际 y={y}"
    );
}

/// 指针停在左侧轨道列时，滚轮同样要能滚动（泳道命中区不覆盖这里）。
#[test]
fn wheel_over_track_list_scrolls() {
    let mut h = harness();
    // 轨道列 x∈[0,160]、y∈[28,560]，避开底部状态栏。
    move_pointer(&mut h.ui, egui::pos2(80.0, 300.0));

    wheel(
        &mut h.ui,
        &[(egui::MouseWheelUnit::Line, egui::vec2(0.0, -1.0))],
        egui::Modifiers::default(),
    );

    let y = scroll_y(&h.lane.borrow()).expect("轨道列滚轮必须产生 ScrollY");
    assert!(
        (y - (LANE_SCROLL_Y + 30.0)).abs() < f32::EPSILON,
        "向下滚 1 行后 scroll_y 应增加 30，实际 {y}"
    );
}

/// 标尺纵向滚轮仍按原语义缩放 X（重构为共享手势解析后不能丢）。
#[test]
fn ruler_vertical_wheel_still_zooms() {
    let mut h = harness();
    // 标尺 x∈[160,780]、y∈[0,28]。
    move_pointer(&mut h.ui, egui::pos2(400.0, 14.0));

    wheel(
        &mut h.ui,
        &[(egui::MouseWheelUnit::Line, egui::vec2(0.0, 1.0))],
        egui::Modifiers::default(),
    );

    let zoom = h
        .chrome
        .borrow()
        .iter()
        .rev()
        .find_map(|action| match action {
            ArrangementAction::ZoomX { zoom, .. } => Some(*zoom),
            _ => None,
        })
        .expect("标尺纵向滚轮应产生 ZoomX");
    assert!(
        (zoom - 0.11).abs() < 1e-6,
        "1 行 = 10% 缩放，0.1 → 0.11，实际 {zoom}"
    );
}
