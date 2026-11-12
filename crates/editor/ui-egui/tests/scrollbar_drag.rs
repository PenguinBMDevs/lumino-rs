//! 滚动条（egui）：拖拽滑块滚动、点击轨道跳转、边缘拖拽缩放。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::scrollbar::{self, ScrollbarAction, ScrollbarModel};

fn model(vertical: bool) -> ScrollbarModel {
    let rect = if vertical {
        egui::Rect::from_min_size(egui::pos2(380.0, 0.0), egui::vec2(20.0, 400.0))
    } else {
        egui::Rect::from_min_size(egui::pos2(0.0, 380.0), egui::vec2(400.0, 20.0))
    };
    ScrollbarModel {
        rect,
        scroll: 0.0,
        max_scroll: 2000.0,
        zoom: 1.0,
        viewport: if vertical { Some(380.0) } else { None },
        thumb_ratio: None,
        vertical,
    }
}

fn harness(
    model: ScrollbarModel,
) -> (
    Harness<'static>,
    Rc<RefCell<Vec<scrollbar::DirectedAction>>>,
) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(500.0, 500.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let _ = scrollbar::show(ui, "test", &model);
            if let Some(action) = scrollbar::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

fn press(harness: &mut Harness<'static>, pos: egui::Pos2) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(pos));
    input.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    *harness.input_mut() = input;
    harness.run();
}

fn move_to(harness: &mut Harness<'static>, pos: egui::Pos2) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(pos));
    *harness.input_mut() = input;
    harness.run();
}

fn release(harness: &mut Harness<'static>, pos: egui::Pos2) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    *harness.input_mut() = input;
    harness.run();
}

fn last_scroll(actions: &Rc<RefCell<Vec<scrollbar::DirectedAction>>>) -> Option<(bool, f32)> {
    actions.borrow().iter().rev().find_map(|a| match a.action {
        ScrollbarAction::Scroll(x) => Some((a.vertical, x)),
        _ => None,
    })
}

/// 拖动滑块：滚动量按「轨道剩余长度」比例映射到最大可滚动范围。
#[test]
fn drag_thumb_scrolls_proportionally() {
    let (mut h, actions) = harness(model(false));
    // 轨道 396px；滑块 = 396 * 400/2000 ≈ 79.2，滑块中心 x ≈ 41.6。
    press(&mut h, egui::pos2(41.6, 390.0));
    move_to(&mut h, egui::pos2(141.6, 390.0));
    let (vertical, x) = last_scroll(&actions).expect("拖拽应产生 Scroll");
    assert!(!vertical, "水平滚动条动作不能标成垂直");
    // delta=100 → 100/(396-79.2) * (2000-396) ≈ 506。
    assert!(
        (x - 506.0).abs() < 12.0,
        "拖动 100px 应滚动约 506px，实际 {x}"
    );
    release(&mut h, egui::pos2(141.6, 390.0));
}

/// 点击轨道：直接跳到对应比例。
#[test]
fn click_track_jumps_to_ratio() {
    let (mut h, actions) = harness(model(false));
    press(&mut h, egui::pos2(300.0, 390.0));
    release(&mut h, egui::pos2(300.0, 390.0));
    let (_, x) = last_scroll(&actions).expect("轨道点击应产生 Scroll");
    assert!(
        (x - 1508.0).abs() < 20.0,
        "点击轨道 300px 应跳到约 1508px，实际 {x}"
    );
}

/// 边缘拖拽：产生 Zoom（起始边向右拖 → 滑块变短、内容放大）。
#[test]
fn edge_drag_zooms() {
    let (mut h, actions) = harness(model(false));
    // 起始边缘：thumb.left = 2，取 4。
    press(&mut h, egui::pos2(4.0, 390.0));
    move_to(&mut h, egui::pos2(34.0, 390.0));
    let zoom = actions.borrow().iter().rev().find_map(|a| match a.action {
        ScrollbarAction::Zoom { zoom, fixed_ratio } => Some((zoom, fixed_ratio)),
        _ => None,
    });
    let (zoom, fixed_ratio) =
        zoom.unwrap_or_else(|| panic!("起始边拖拽应产生 Zoom，实际动作：{:?}", actions.borrow()));
    assert!(zoom > 1.0, "向右拖起始边应放大，实际 zoom={zoom}");
    assert!(
        (fixed_ratio - 1.0).abs() < f32::EPSILON,
        "起始边固定比例应为 1.0，实际 {fixed_ratio}"
    );
    release(&mut h, egui::pos2(34.0, 390.0));
}

/// 垂直滚动条：拖拽产生带 `vertical = true` 的 Scroll。
#[test]
fn vertical_drag_is_marked_vertical() {
    let (mut h, actions) = harness(model(true));
    // 轨道 396px；滑块 = 396 * 380/2000 ≈ 75.2，中心 y ≈ 39.6。
    press(&mut h, egui::pos2(390.0, 39.6));
    move_to(&mut h, egui::pos2(390.0, 139.6));
    let (vertical, y) = last_scroll(&actions).expect("垂直拖拽应产生 Scroll");
    assert!(vertical, "垂直滚动条动作必须标成垂直");
    assert!(y > 400.0, "向下拖 100px 应滚动数百像素，实际 {y}");
    release(&mut h, egui::pos2(390.0, 139.6));
}
