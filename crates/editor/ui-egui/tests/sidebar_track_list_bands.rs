//! 左侧栏音轨列表：行点击 / 添加行点击 / 命中区占用。
//!
//! 真机回归点：音轨行原先只有 `Sense::hover()`，点行选不中音轨；添加行点击会
//! 同时被 iced 处理（一次点击加两条音轨）。这里把两件事都钉死。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::sidebar::{self, SidebarPanelAction};
use lumino_ui_egui::chrome::{SidebarGroup, SidebarModel, SidebarPanel, SidebarTrack, menu};
use lumino_ui_egui::layout;

fn track(id: usize, label: &str, name: &str, selected: bool) -> SidebarTrack {
    SidebarTrack {
        id,
        label: label.to_owned(),
        name: name.to_owned(),
        color: None,
        selected,
        conductor: false,
        soloed: false,
    }
}

fn model() -> SidebarModel {
    SidebarModel {
        panel_visible: true,
        panel_width: 200.0,
        active_group: Some(SidebarGroup::PianoRoll),
        file_active: true,
        panel: SidebarPanel::Tracks,
        // ID 故意不是 0..n，验证回传的是音轨 ID 而不是下标。
        tracks: vec![
            track(7, "A01", "Setup", false),
            track(3, "A02", "Piano", true),
            track(11, "A03", "Strings", false),
        ],
        track_list_title: "音轨列表".to_owned(),
        add_track_label: "添加音轨".to_owned(),
        ..SidebarModel::default()
    }
}

fn harness(model: SidebarModel) -> (Harness<'static>, Rc<RefCell<Vec<SidebarPanelAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(layout::SIDEBAR_W, 600.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            sidebar::show_model(ui, &model, &mut icons);
            if let Some(action) = sidebar::take_panel_action(ui.ctx()) {
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

fn row_center(index: usize) -> Pos2 {
    let top = layout::TRACK_LIST_HEADER_H + layout::TRACK_ROW_H * index as f32;
    Pos2::new(
        layout::ROUTE_BAR_W + 8.0 + 80.0,
        top + layout::TRACK_ROW_H / 2.0,
    )
}

#[test]
fn clicking_a_row_reports_its_track_id() {
    for (index, id) in [(0usize, 7usize), (1, 3), (2, 11)] {
        let (mut h, actions) = harness(model());
        click(&mut h, row_center(index));
        assert_eq!(
            actions.borrow().last(),
            Some(&SidebarPanelAction::SelectTrack(id)),
            "第 {index} 行应回传音轨 ID {id}（不是下标）"
        );
    }
}

#[test]
fn clicking_add_row_requests_one_track() {
    let (mut h, actions) = harness(model());
    let top = layout::TRACK_LIST_HEADER_H + layout::TRACK_ROW_H * 3.0;
    click(
        &mut h,
        Pos2::new(
            layout::ROUTE_BAR_W + 8.0 + 80.0,
            top + layout::TRACK_ADD_ROW_H / 2.0,
        ),
    );
    assert_eq!(
        actions.borrow().as_slice(),
        &[SidebarPanelAction::AddTrack],
        "点添加行必须**恰好**产生一次 AddTrack（否则会加两条音轨）"
    );
}

#[test]
fn rows_and_add_row_are_blocked_from_iced() {
    let (h, _) = harness(model());
    let blocked = menu::take_blocked(&h.ctx);
    assert!(
        blocked.iter().any(|r| r.contains(row_center(0))),
        "音轨行必须进入占用区"
    );
    let add_y =
        layout::TRACK_LIST_HEADER_H + layout::TRACK_ROW_H * 3.0 + layout::TRACK_ADD_ROW_H / 2.0;
    assert!(
        blocked
            .iter()
            .any(|r| r.contains(Pos2::new(layout::ROUTE_BAR_W + 80.0, add_y))),
        "添加行必须进入占用区"
    );
}

#[test]
fn blank_area_of_panel_yields_no_action() {
    let (mut h, actions) = harness(SidebarModel {
        panel: SidebarPanel::Blank,
        ..model()
    });
    click(&mut h, Pos2::new(layout::ROUTE_BAR_W + 80.0, 300.0));
    assert!(actions.borrow().is_empty(), "空白面板不应产生动作");
}
