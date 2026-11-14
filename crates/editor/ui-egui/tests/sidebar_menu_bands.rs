//! 左侧栏右键菜单：菜单项点击 / 背景关闭 / 占用登记。
//!
//! 几何与 旧版 一致：面板内容宽 = panel_width − 6，菜单右对齐 −8（音轨）/ −4（面板），
//! 按钮 40、间距 4、内边距 8 → 菜单宽 56。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::sidebar::{self, SidebarMenuAction};
use lumino_ui_egui::chrome::{
    SidebarGroup, SidebarMenuEntry, SidebarModel, SidebarPanel, SidebarPanelMenuModel,
    SidebarTrack, SidebarTrackMenuModel, menu,
};
use lumino_ui_egui::icons::Icon;
use lumino_ui_egui::layout;

fn track(id: usize, label: &str, name: &str) -> SidebarTrack {
    SidebarTrack {
        id,
        label: label.to_owned(),
        name: name.to_owned(),
        color: None,
        selected: id == 3,
        conductor: false,
        soloed: false,
        renaming: false,
        rename_buffer: String::new(),
    }
}

fn entries() -> Vec<SidebarMenuEntry> {
    [
        Icon::ContextMenuDelete,
        Icon::PencilOutline,
        Icon::ContextMenuColorPalette,
        Icon::ContextMenuChannel,
        Icon::ContextMenuPort,
    ]
    .into_iter()
    .map(|icon| SidebarMenuEntry {
        icon,
        label: "菜单项".to_owned(),
    })
    .collect()
}

fn base() -> SidebarModel {
    SidebarModel {
        panel_visible: true,
        panel_width: 200.0,
        active_group: Some(SidebarGroup::PianoRoll),
        file_active: true,
        panel: SidebarPanel::Tracks,
        tracks: vec![track(7, "A01", "Setup"), track(3, "A02", "Piano")],
        track_list_title: "音轨列表".to_owned(),
        add_track_label: "添加音轨".to_owned(),
        ..SidebarModel::default()
    }
}

fn harness(model: SidebarModel) -> (Harness<'static>, Rc<RefCell<Vec<SidebarMenuAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(layout::SIDEBAR_W, 600.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            sidebar::show_model(ui, &model, &mut icons);
            if let Some(action) = sidebar::take_menu_action(ui.ctx()) {
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

/// 菜单左边界：面板内容右缘 − 8 − 56。
fn menu_left() -> f32 {
    let content_right = layout::ROUTE_BAR_W + 200.0 - 6.0;
    content_right - 8.0 - 56.0
}

/// 第 `index` 个按钮中心：菜单左上 + 8 内边距 + index ×（40 + 4）+ 20。
fn item_center(menu_top: f32, index: usize) -> Pos2 {
    Pos2::new(
        menu_left() + 28.0,
        menu_top + 8.0 + index as f32 * 44.0 + 20.0,
    )
}

#[test]
fn clicking_track_menu_item_reports_track_and_index() {
    // 第二行（track id 3）的顶部 = 标题 24 + 行高 36；菜单顶部 = 行顶 + 8。
    let menu_top = layout::TRACK_LIST_HEADER_H + layout::TRACK_ROW_H + 8.0;
    for index in 0..5 {
        let (mut h, actions) = harness(SidebarModel {
            context_menu: Some(SidebarTrackMenuModel {
                track_id: 3,
                entries: entries(),
            }),
            ..base()
        });
        click(&mut h, item_center(menu_top, index));
        assert_eq!(
            actions.borrow().last(),
            Some(&SidebarMenuAction::TrackItem { track_id: 3, index }),
            "第 {index} 项应回传目标音轨与下标"
        );
    }
}

#[test]
fn clicking_outside_track_menu_closes_it() {
    let (mut h, actions) = harness(SidebarModel {
        context_menu: Some(SidebarTrackMenuModel {
            track_id: 3,
            entries: entries(),
        }),
        ..base()
    });
    click(&mut h, Pos2::new(layout::ROUTE_BAR_W + 20.0, 500.0));
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarMenuAction::CloseTrackMenu),
        "点面板空白应关闭音轨菜单"
    );
}

#[test]
fn panel_menu_item_and_close() {
    let model = SidebarModel {
        panel_menu: Some(SidebarPanelMenuModel {
            pos: egui::pos2(100.0, 120.0),
            entries: entries()[..1].to_vec(),
        }),
        ..base()
    };
    let (mut h, actions) = harness(model.clone());
    click(
        &mut h,
        Pos2::new(menu_left() + 28.0, 120.0 + 4.0 + 8.0 + 20.0),
    );
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarMenuAction::PanelItem(0))
    );

    let (mut h, actions) = harness(model);
    click(&mut h, Pos2::new(layout::ROUTE_BAR_W + 20.0, 500.0));
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarMenuAction::ClosePanelMenu)
    );
}

#[test]
fn escape_closes_track_menu() {
    let (mut h, actions) = harness(SidebarModel {
        context_menu: Some(SidebarTrackMenuModel {
            track_id: 3,
            entries: entries(),
        }),
        ..base()
    });
    *h.input_mut() = RawInput {
        events: vec![Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
        ..Default::default()
    };
    h.run();
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarMenuAction::CloseTrackMenu),
        "Esc 应关闭音轨菜单"
    );
}

/// 菜单本体必须被 egui 的**图层**独占（不再是手工占用矩形）。
///
/// Step 7 把左侧栏菜单的全局占用登记删掉了——菜单走独立 `Area`，
/// `Host::egui_layer_owns_pos` 经 `ctx.layer_id_at` 已能独占命中。
/// 这条断言比"手工矩形里有一项"更强：它验证的是真实命中栈，
/// 一旦菜单退回 root 绘制（悬停/点击穿透），这里会先红。
#[test]
fn menu_rect_is_owned_by_foreground_layer() {
    let (h, _) = harness(SidebarModel {
        context_menu: Some(SidebarTrackMenuModel {
            track_id: 3,
            entries: entries(),
        }),
        ..base()
    });
    let menu_top = layout::TRACK_LIST_HEADER_H + layout::TRACK_ROW_H + 8.0;
    let probe = Pos2::new(menu_left() + 28.0, menu_top + 20.0);
    let layer = h.ctx.layer_id_at(probe).expect("菜单图层应存在");
    assert_ne!(
        layer.order,
        egui::Order::Background,
        "菜单本体必须由独立图层独占命中（否则同一次点击会穿透到画布）"
    );
    // 注意：探针同时落在下层音轨行内，而音轨行由 root 绘制、仍需登记占用
    //（那是管用的一份）。这里只断言**菜单本体**（宽 56）不再登记。
    assert!(
        !menu::take_blocked(&h.ctx)
            .iter()
            .any(|r| r.contains(probe) && (r.width() - 56.0).abs() < 1.0),
        "Step 7 后菜单不再登记全局占用表（命中已归图层）"
    );
}
