//! 悬浮绘制工具条 + 锚定面板的无头交互测试。
//!
//! 验证「状态进来、动作出去」：条目 / 粗细 / 齿轮 / 形状选择 / 面板关闭 / 拖拽，
//! 像素级外观由真实窗口截图负责。

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{
    self, ChromeModel, FillDivisionPanelModel, ShapeKind, ShapePanelModel, StatusbarModel,
    ThicknessPanelModel, ToolPanelItemKind, ToolPanelItemModel, ToolPanelModel,
    ToolPanelSettingsPanel, ToolbarAction,
};
use lumino_ui_egui::icons::{Icon, IconCache};

fn statusbar() -> StatusbarModel {
    StatusbarModel {
        left: "就绪".to_owned(),
        left_accent: true,
        cpu_percent: Some(1.0),
        memory_mb: 256.0,
        fps: Some(60.0),
        right: String::new(),
    }
}

fn item(kind: ToolPanelItemKind, settings: bool) -> ToolPanelItemModel {
    ToolPanelItemModel {
        kind,
        icon: Icon::MousePointer,
        tooltip: format!("{kind:?}"),
        selected: false,
        settings_request: settings,
    }
}

fn panel() -> ToolPanelModel {
    ToolPanelModel {
        offset: (0.0, 44.0),
        left_inset: 0.0,
        top_inset: 102.0,
        thickness_tooltip: "粗细".to_owned(),
        items: vec![
            item(ToolPanelItemKind::Mouse, false),
            item(ToolPanelItemKind::Curve, false),
            item(ToolPanelItemKind::FillBucket, false),
            item(ToolPanelItemKind::Brush, true),
            item(ToolPanelItemKind::Shape, true),
            item(ToolPanelItemKind::Text, true),
        ],
        thickness_selected: false,
        panel: None,
    }
}

fn click(harness: &mut Harness<'static>, pos: egui::Pos2) {
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(pos));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
}

/// 悬浮条条目 / 粗细 / 齿轮的点击路由。
#[test]
fn tool_panel_dispatches_items_and_settings() {
    let model = std::rc::Rc::new(std::cell::RefCell::new(ChromeModel {
        tool_panel: Some(panel()),
        statusbar: statusbar(),
        ..ChromeModel::default()
    }));
    let actions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut icons = IconCache::default();
    let model_w = model.clone();
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model_w.borrow(), &mut icons);
            if let Some(action) = chrome::toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();

    let pill = chrome::tool_panel::pill_rect(&harness.ctx).expect("应记录胶囊矩形");
    assert!(pill.width() > 300.0, "胶囊宽度应按条目累加");
    let curve =
        chrome::tool_panel::item_rect(&harness.ctx, ToolPanelItemKind::Curve).expect("曲线条目");
    click(&mut harness, curve.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::ToolPanelItem {
            kind: ToolPanelItemKind::Curve,
            settings: false,
        }),
        "未启用条目 → 普通选择"
    );

    let brush =
        chrome::tool_panel::item_rect(&harness.ctx, ToolPanelItemKind::Brush).expect("画刷条目");
    click(&mut harness, brush.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::ToolPanelItem {
            kind: ToolPanelItemKind::Brush,
            settings: true,
        }),
        "已启用且有设置 → 弹面板"
    );

    let thickness = chrome::tool_panel::thickness_rect(&harness.ctx).expect("粗细按钮");
    click(&mut harness, thickness.center());
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::ThicknessToggle),
        "粗细按钮 → 开合面板"
    );

    let gear = egui::pos2(thickness.right() + 4.0 + 17.0, thickness.center().y);
    click(&mut harness, gear);
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::OpenDrawSettings),
        "齿轮 → 打开音符画设置窗口"
    );
}

/// 形状面板：选择图形 + 形状抽屉入口。
#[test]
fn shape_panel_selects_kind_and_opens_drawer() {
    let mut model = panel();
    model.panel = Some(ToolPanelSettingsPanel::Shape(ShapePanelModel {
        selected: ShapeKind::Rectangle,
        rectangle: "矩形".to_owned(),
        circle: "圆形".to_owned(),
        triangle: "三角形".to_owned(),
        drawer: "形状抽屉".to_owned(),
    }));
    let model = std::rc::Rc::new(std::cell::RefCell::new(ChromeModel {
        tool_panel: Some(model),
        statusbar: statusbar(),
        ..ChromeModel::default()
    }));
    let actions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut icons = IconCache::default();
    let model_w = model.clone();
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model_w.borrow(), &mut icons);
            if let Some(action) = chrome::toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();

    let panel = {
        let model = model.borrow();
        let panel_model = model
            .tool_panel
            .as_ref()
            .and_then(|tool| tool.panel.as_ref())
            .expect("形状面板");
        chrome::tool_panel::panel_rect_for(&harness.ctx, panel_model).expect("面板矩形")
    };
    let y = panel.top() + 8.0 + 20.0;
    click(
        &mut harness,
        egui::pos2(panel.left() + 8.0 + 40.0 + 4.0 + 20.0, y),
    );
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::ShapeSelected(ShapeKind::Circle)),
        "第二个按钮 → 圆形"
    );

    click(
        &mut harness,
        egui::pos2(panel.left() + 8.0 + 3.0 * 44.0 + 20.0, y),
    );
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::OpenShapeDrawer),
        "第四个按钮 → 形状抽屉"
    );

    // 面板外点击：关闭当前面板（动作不落到胶囊 / 面板）。
    click(&mut harness, egui::pos2(160.0, 700.0));
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::CloseShapePanel),
        "面板外点击应请求关闭形状面板"
    );
}

/// 粗细面板：滑杆回传整数；面板外点击关闭。
#[test]
fn thickness_panel_closes_on_outside_click() {
    let mut model = panel();
    model.panel = Some(ToolPanelSettingsPanel::Thickness(ThicknessPanelModel {
        value: 20,
        min: 1,
        max: 100,
        title: "粗细".to_owned(),
        value_text: "20 层 (1 - 100)".to_owned(),
        hint: "说明".to_owned(),
    }));
    model.thickness_selected = true;
    let model = std::rc::Rc::new(std::cell::RefCell::new(ChromeModel {
        tool_panel: Some(model),
        statusbar: statusbar(),
        ..ChromeModel::default()
    }));
    let actions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut icons = IconCache::default();
    let model_w = model.clone();
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model_w.borrow(), &mut icons);
            if let Some(action) = chrome::toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();

    click(&mut harness, egui::pos2(600.0, 750.0));
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::CloseThicknessPanel),
        "面板外点击应请求关闭粗细面板"
    );
}

/// 分音符填充输入框：点击聚焦后，键盘文本必须回传生产。
#[test]
fn fill_input_accepts_typed_text() {
    let mut model = panel();
    model.panel = Some(ToolPanelSettingsPanel::FillDivision(
        FillDivisionPanelModel {
            value: String::new(),
            error: None,
            prefix: "使用".to_owned(),
            suffix: "分音符填充".to_owned(),
            hint: "留空 = 整块填充".to_owned(),
            disable: "关闭填充".to_owned(),
            cancel: "取消".to_owned(),
            ok: "确定".to_owned(),
        },
    ));
    let model = std::rc::Rc::new(std::cell::RefCell::new(ChromeModel {
        tool_panel: Some(model),
        statusbar: statusbar(),
        ..ChromeModel::default()
    }));
    let actions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut icons = IconCache::default();
    let model_w = model.clone();
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model_w.borrow(), &mut icons);
            if let Some(action) = chrome::toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();

    let input = chrome::tool_panel::fill_input_rect(&harness.ctx).expect("填充输入框矩形");
    click(&mut harness, input.center());
    assert!(
        harness.ctx.text_edit_focused(),
        "点击输入框后应获得键盘焦点"
    );

    harness
        .input_mut()
        .events
        .push(egui::Event::Text("16".to_owned()));
    harness.run();
    assert!(
        actions.borrow().iter().any(
            |action| matches!(action, ToolbarAction::FillDivisionChanged(text) if text == "16")
        ),
        "键入 16 应回传 FillDivisionChanged，实际：{:?}",
        actions.borrow()
    );
}

/// 拖拽柄：按下并移动 → `ToolPanelDragStarted` / `ToolPanelDragged`，松手 → `DragEnded`。
#[test]
fn grip_drag_dispatches_start_move_end() {
    let model = ChromeModel {
        tool_panel: Some(panel()),
        statusbar: statusbar(),
        ..ChromeModel::default()
    };
    let actions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut icons = IconCache::default();
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = chrome::toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();

    let pill = chrome::tool_panel::pill_rect(&harness.ctx).expect("胶囊");
    let grip = egui::pos2(pill.left() + 5.0 + 11.0, pill.center().y);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(grip));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: grip,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(grip + egui::vec2(24.0, -10.0)));
    harness.run();
    assert!(
        actions
            .borrow()
            .iter()
            .any(|action| matches!(action, ToolbarAction::ToolPanelDragStarted)),
        "按下并移动应起拖，实际：{:?}",
        actions.borrow()
    );

    let end = grip + egui::vec2(24.0, -10.0);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
    assert!(
        actions
            .borrow()
            .iter()
            .any(|action| matches!(action, ToolbarAction::ToolPanelDragEnded)),
        "松手应结束拖拽，实际：{:?}",
        actions.borrow()
    );
    assert_eq!(
        actions.borrow().last(),
        Some(&ToolbarAction::ToolPanelDragEnded),
        "释放帧必须回传 DragEnded（不能被同帧 Dragged 抢走）"
    );
}
