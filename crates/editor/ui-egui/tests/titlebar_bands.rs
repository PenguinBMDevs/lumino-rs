//! 标题栏动作：主窗口（拖动 / 双击 / 红绿灯）与对话框 popup 标题栏。
//!
//! 上一代 标题栏已退化为 30px 占位，行为全部由 egui 产生并经
//! `TitlebarAction` 回传；这里钉住每个手势 → 语义动作的映射。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{
    self, ChromeModel, PopupTitlebarModel, TitlebarAction, TitlebarModel,
};

fn main_model() -> ChromeModel {
    ChromeModel {
        titlebar: Some(TitlebarModel {
            focused: true,
            menus: vec![
                "文件".to_owned(),
                "编辑".to_owned(),
                "视图".to_owned(),
                "帮助".to_owned(),
            ],
            menu: None,
            menu_only: false,
        }),
        ..ChromeModel::default()
    }
}

fn popup_model() -> ChromeModel {
    ChromeModel {
        dialog_titlebar: Some(PopupTitlebarModel { focused: true }),
        ..ChromeModel::default()
    }
}

fn harness(model: ChromeModel) -> (Harness<'static>, Rc<RefCell<Vec<TitlebarAction>>>) {
    harness_with_step_dt(model, 0.25)
}

/// 与 [`harness`] 相同，但可指定模拟步长。
///
/// egui 的双击判定窗口是 300ms（`max_double_click_delay`），kittest 默认步长
/// 250ms 会把两次点击推到窗口之外；双击手势测试需要更细的步长。
fn harness_with_step_dt(
    model: ChromeModel,
    step_dt: f32,
) -> (Harness<'static>, Rc<RefCell<Vec<TitlebarAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .with_step_dt(step_dt)
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = chrome::take_titlebar_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

/// 标题栏 logo 的交互矩形（`titlebar::menu` 用固定 id 注册）。
fn logo_rect(h: &Harness<'static>) -> egui::Rect {
    h.ctx
        .read_response(egui::Id::unique("lumino.titlebar.logo"))
        .expect("标题栏 logo 应注册可交互矩形")
        .rect
}

fn press(harness: &mut Harness<'static>, pos: egui::Pos2) {
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
    harness.run();
}

fn release(harness: &mut Harness<'static>, pos: egui::Pos2) {
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
}

fn click(harness: &mut Harness<'static>, pos: egui::Pos2) {
    press(harness, pos);
    release(harness, pos);
}

/// 红绿灯：最右侧为关闭（45×29）。
fn close_center() -> egui::Pos2 {
    egui::pos2(1600.0 - 22.0, 15.0)
}

fn max_center() -> egui::Pos2 {
    egui::pos2(1600.0 - 67.0, 15.0)
}

fn min_center() -> egui::Pos2 {
    egui::pos2(1600.0 - 112.0, 15.0)
}

#[test]
fn main_titlebar_drag_starts_window_drag() {
    let (mut h, actions) = harness(main_model());
    press(&mut h, egui::pos2(800.0, 15.0));
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(840.0, 15.0)));
    h.run();
    assert!(
        actions.borrow().contains(&TitlebarAction::Drag),
        "拖动标题栏应回传 Drag：{:?}",
        actions.borrow()
    );
}

#[test]
fn main_titlebar_traffic_buttons_emit_actions() {
    for (pos, expected) in [
        (close_center(), TitlebarAction::Close),
        (max_center(), TitlebarAction::ToggleMaximize),
        (min_center(), TitlebarAction::Minimize),
    ] {
        let (mut h, actions) = harness(main_model());
        click(&mut h, pos);
        assert_eq!(
            actions.borrow().last(),
            Some(&expected),
            "点 {pos:?} 应回传 {expected:?}"
        );
    }
}

#[test]
fn logo_double_click_emits_close() {
    // 5ms 步长保证两次点击都落在 egui 的 300ms 双击窗口内。
    let (mut h, actions) = harness_with_step_dt(main_model(), 0.005);
    let center = logo_rect(&h).center();
    click(&mut h, center);
    click(&mut h, center);
    assert_eq!(
        actions.borrow().last(),
        Some(&TitlebarAction::Close),
        "双击标题栏 logo 应回传 Close（与红叉同一关闭链路）：{:?}",
        actions.borrow()
    );
}

#[test]
fn logo_single_click_does_nothing() {
    let (mut h, actions) = harness_with_step_dt(main_model(), 0.005);
    let center = logo_rect(&h).center();
    click(&mut h, center);
    assert!(
        actions.borrow().is_empty(),
        "单击 logo 不应产生任何标题栏动作（不拖动 / 不最大化 / 不关闭）：{:?}",
        actions.borrow()
    );
}

#[test]
fn hover_tooltip_does_not_break_traffic_click() {
    let (mut h, actions) = harness(main_model());
    // 先把指针停在关闭按钮上，让悬停说明真正弹出，再点击。
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(close_center()));
    h.run();
    h.run();
    click(&mut h, close_center());
    assert_eq!(
        actions.borrow().last(),
        Some(&TitlebarAction::Close),
        "悬停说明弹出后，关闭按钮仍必须可点击"
    );
}

#[test]
fn popup_titlebar_drag_and_close() {
    // 拖动区（红绿灯左侧）
    let (mut h, actions) = harness(popup_model());
    press(&mut h, egui::pos2(400.0, 15.0));
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(440.0, 15.0)));
    h.run();
    assert!(
        actions.borrow().contains(&TitlebarAction::Drag),
        "对话框标题栏拖动应回传 Drag：{:?}",
        actions.borrow()
    );

    let (mut h, actions) = harness(popup_model());
    click(&mut h, close_center());
    assert_eq!(
        actions.borrow().last(),
        Some(&TitlebarAction::Close),
        "对话框关闭按钮应回传 Close"
    );
}

#[test]
fn popup_titlebar_absent_when_dialog_draws_its_own() {
    // 模型里没有 dialog_titlebar 时不得画 30px 条（设置 / 保存确认自带标题栏）。
    let (h, _) = harness(ChromeModel {
        dialog_titlebar: None,
        ..main_model()
    });
    // 交互层不产生任何标题栏动作（这里只验证不 panic 且无动作）。
    let actions = h
        .ctx
        .data(|d| d.get_temp::<Option<TitlebarAction>>(egui::Id::unique("lumino.titlebar.action")));
    assert!(actions.flatten().is_none());
}
