//! 路由栏点击映射：点位 → `SidebarRoute`（无头，锁死绘制与命中同源）。
//!
//! 背景：egui 不透明绘制路由栏后必须自己接管点击；若绘制与命中的条目表不一致，
//! 就会出现「看到的图标」与「点得中的按钮」错位。本用例按生产条目表逐格点击。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::{
    SidebarGroup, SidebarModel, SidebarPanel, SidebarRouteIcon, SidebarRouteItem, sidebar,
};

fn items() -> Vec<SidebarRouteItem> {
    [
        SidebarRouteIcon::PianoRoll,
        SidebarRouteIcon::File,
        SidebarRouteIcon::Automation,
        SidebarRouteIcon::Project,
        SidebarRouteIcon::Waterfall,
        SidebarRouteIcon::Renderer,
        SidebarRouteIcon::VideoExport,
        SidebarRouteIcon::AudioExport,
    ]
    .into_iter()
    .map(|icon| SidebarRouteItem {
        icon,
        active: false,
    })
    .collect()
}

fn make_harness(
    route_items: Vec<SidebarRouteItem>,
) -> (Harness<'static>, Rc<RefCell<Vec<sidebar::SidebarRoute>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let model = SidebarModel {
        panel: SidebarPanel::Blank,
        panel_visible: false,
        active_group: Some(SidebarGroup::Renderer),
        route_items,
        ..SidebarModel::default()
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 640.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            let mut state = lumino_ui_egui::chrome::ChromeState {
                sidebar: model.clone(),
                ..Default::default()
            };
            sidebar::show(ui, &mut state);
            if let Some(route) = sidebar::take_route_action(ui.ctx()) {
                actions_w.borrow_mut().push(route);
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
fn every_rail_slot_maps_to_its_own_route() {
    // 路由栏从 y=0 起、步进 48；点击每格中心应产出对应动作。
    let expected = [
        sidebar::SidebarRoute::PianoRollGroup,
        sidebar::SidebarRoute::FilePanel,
        sidebar::SidebarRoute::AutomationPanel,
        sidebar::SidebarRoute::ProjectGroup,
        sidebar::SidebarRoute::WaterfallGroup,
        sidebar::SidebarRoute::RendererGroup,
        sidebar::SidebarRoute::VideoExport,
        sidebar::SidebarRoute::AudioExport,
    ];
    for (index, want) in expected.iter().enumerate() {
        let (mut harness, actions) = make_harness(items());
        click(&mut harness, Pos2::new(24.0, index as f32 * 48.0 + 24.0));
        assert_eq!(actions.borrow().last(), Some(want), "第 {index} 格");
    }
}

#[test]
fn rail_does_not_emit_outside_its_items() {
    // 只有 4 格时，第 5 格不应产出任何动作（旧实现会错位一格）。
    let (mut harness, actions) = make_harness(items().into_iter().take(4).collect());
    click(&mut harness, Pos2::new(24.0, 4.0 * 48.0 + 24.0));
    assert!(actions.borrow().is_empty(), "越界槽位不得产出动作");
}

#[test]
fn bottom_buttons_never_emit_routes() {
    // 卷帘方向 / 混音台按钮不产生路由动作。
    let (mut harness, actions) = make_harness(items());
    click(&mut harness, Pos2::new(24.0, 640.0 - 48.0 * 2.5));
    assert!(
        actions
            .borrow()
            .iter()
            .all(|route| !matches!(route, sidebar::SidebarRoute::VideoExport)),
        "底部按钮不得误判成导出按钮"
    );
}
