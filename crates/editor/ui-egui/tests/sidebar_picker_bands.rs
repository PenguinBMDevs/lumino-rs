//! 左侧栏取色器 / 端口·通道选择器：命中、当前值、背景关闭、占用。
//!
//! 几何与 旧版 一致：右对齐到内容右缘 −8、纵向贴目标行 +8；
//! 取色器色块 20/间距 4/内边距 8（8 色 + 恢复默认）；
//! 数值按钮 30/间距 4/内边距 8（16 项）。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::sidebar::{self, SidebarPanelAction};
use lumino_ui_egui::chrome::{
    SidebarColorPickerModel, SidebarGroup, SidebarModel, SidebarNumericPickerKind,
    SidebarNumericPickerModel, SidebarPanel, SidebarTrack, menu,
};
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

/// 面板内容右缘（48 + 200 − 6）。
fn content_right() -> f32 {
    layout::ROUTE_BAR_W + 200.0 - 6.0
}

/// 第二行（track 3）顶部 + 8。
fn picker_top() -> f32 {
    layout::TRACK_LIST_HEADER_H + layout::TRACK_ROW_H + 8.0
}

#[test]
fn color_picker_swatches_and_reset_emit_actions() {
    let size = 4.0 * 20.0 + 3.0 * 4.0 + 16.0;
    let left = content_right() - 8.0 - size;
    let top = picker_top();
    let swatch = |index: usize| {
        Pos2::new(
            left + 8.0 + (index % 4) as f32 * 24.0 + 10.0,
            top + 8.0 + (index / 4) as f32 * 24.0 + 10.0,
        )
    };

    let model = SidebarModel {
        color_picker: Some(SidebarColorPickerModel { track_id: 3 }),
        ..base()
    };
    // 第 1 个色块 = 红 (217, 38, 38)。
    let (mut h, actions) = harness(model.clone());
    click(&mut h, swatch(0));
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarPanelAction::ColorPicked {
            track_id: 3,
            rgb: (217, 38, 38)
        })
    );

    // 第 8 个色块 = 灰 (128,128,128)。
    let (mut h, actions) = harness(model.clone());
    click(&mut h, swatch(7));
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarPanelAction::ColorPicked {
            track_id: 3,
            rgb: (128, 128, 128)
        })
    );

    // 第 9 格 = 恢复默认。
    let (mut h, actions) = harness(model.clone());
    click(&mut h, swatch(8));
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarPanelAction::ColorReset(3))
    );

    // 面板外点击 → 关闭。
    let (mut h, actions) = harness(model);
    click(&mut h, Pos2::new(layout::ROUTE_BAR_W + 20.0, 500.0));
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarPanelAction::ColorPickerClosed(3))
    );
}

#[test]
fn numeric_picker_emits_zero_based_value() {
    let size = 4.0 * 30.0 + 3.0 * 4.0 + 16.0;
    let left = content_right() - 8.0 - size;
    let top = picker_top();
    let cell = |value: usize| {
        Pos2::new(
            left + 8.0 + (value % 4) as f32 * 34.0 + 15.0,
            top + 8.0 + (value / 4) as f32 * 34.0 + 15.0,
        )
    };

    let model = SidebarModel {
        numeric_picker: Some(SidebarNumericPickerModel {
            track_id: 3,
            kind: SidebarNumericPickerKind::Port,
            current: 4,
            choices: 16,
        }),
        ..base()
    };
    // 显示值 = 内部值 + 1：点第 6 个格子 → 内部值 5。
    let (mut h, actions) = harness(model.clone());
    click(&mut h, cell(5));
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarPanelAction::PortPicked {
            track_id: 3,
            value: 5
        })
    );

    let (mut h, actions) = harness(model);
    click(&mut h, Pos2::new(layout::ROUTE_BAR_W + 20.0, 500.0));
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarPanelAction::PortPickerClosed(3))
    );
}

#[test]
fn channel_picker_reports_channel_variant() {
    let size = 4.0 * 30.0 + 3.0 * 4.0 + 16.0;
    let left = content_right() - 8.0 - size;
    let top = picker_top();
    let (mut h, actions) = harness(SidebarModel {
        numeric_picker: Some(SidebarNumericPickerModel {
            track_id: 3,
            kind: SidebarNumericPickerKind::Channel,
            current: 0,
            choices: 16,
        }),
        ..base()
    });
    click(&mut h, Pos2::new(left + 8.0 + 15.0, top + 8.0 + 15.0));
    assert_eq!(
        actions.borrow().last(),
        Some(&SidebarPanelAction::ChannelPicked {
            track_id: 3,
            value: 0
        })
    );
}

/// 选择器面板必须被 egui 的**图层**独占（不再是手工占用矩形）。
///
/// 与左侧栏菜单同步：Step 7 删除全局登记，命中归 `Area` 图层（`layer_id_at`）。
#[test]
fn picker_rect_is_owned_by_foreground_layer() {
    let size = 4.0 * 30.0 + 3.0 * 4.0 + 16.0;
    let left = content_right() - 8.0 - size;
    let top = picker_top();
    let (h, _) = harness(SidebarModel {
        numeric_picker: Some(SidebarNumericPickerModel {
            track_id: 3,
            kind: SidebarNumericPickerKind::Port,
            current: 0,
            choices: 16,
        }),
        ..base()
    });
    let probe = Pos2::new(left + 20.0, top + 20.0);
    let layer = h.ctx.layer_id_at(probe).expect("选择器图层应存在");
    assert_ne!(
        layer.order,
        egui::Order::Background,
        "选择器面板必须由独立图层独占命中"
    );
    // 探针可能同时落在下层音轨行内（那是由 root 绘制、仍需登记的一份）。
    // 这里只断言**选择器本体**（宽 = size）不再登记。
    assert!(
        !menu::take_blocked(&h.ctx)
            .iter()
            .any(|r| r.contains(probe) && (r.width() - size).abs() < 1.0),
        "Step 7 后选择器不再登记全局占用表（命中已归图层）"
    );
}
