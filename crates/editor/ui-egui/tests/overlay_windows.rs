//! 辅助窗口（进度 / 设备警告）的 egui 骨架：动作与勾选状态。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{DeviceWarningModel, ProgressModel};
use lumino_ui_egui::overlay_window::{self, OverlayWindowAction};

fn progress() -> ProgressModel {
    ProgressModel {
        titlebar: false,
        title: "处理中...".into(),
        message: "正在载入 MIDI".into(),
        progress: 0.35,
    }
}

fn warning(suppress: bool) -> DeviceWarningModel {
    DeviceWarningModel {
        titlebar: false,
        heading: "你的电脑不满足运行 Lumino 的最低要求".into(),
        detail: "缺少 GPU 特性：TIMESTAMP_QUERY".into(),
        suppress,
        suppress_label: "不要再提示我".into(),
        quit_label: "确认并关闭".into(),
        continue_label: "放我进去".into(),
    }
}

fn harness_warning(
    model: DeviceWarningModel,
) -> (Harness<'static>, Rc<RefCell<Vec<OverlayWindowAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let sink = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 360.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            overlay_window::show_device_warning(ui, &model);
            if let Some(action) = overlay_window::take_action(ui.ctx()) {
                sink.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

/// 进度窗：三行内容都能画出（不 panic，进度条存在）。
#[test]
fn progress_window_renders() {
    let model = progress();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 360.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| overlay_window::show_progress(ui, &model));
    harness.run();
    let _ = harness.get_by_label("处理中...");
    let _ = harness.get_by_label("正在载入 MIDI");
}

/// 设备警告窗：「放我进去」携带当前勾选状态。
#[test]
fn device_warning_continue_carries_suppress() {
    let (mut harness, actions) = harness_warning(warning(true));
    harness.get_by_label("放我进去").click();
    harness.run();
    assert_eq!(
        actions.borrow().last().copied(),
        Some(OverlayWindowAction::Continue { suppress: true }),
        "点击「放我进去」应发 Continue{{suppress:true}}，实际：{:?}",
        actions.borrow()
    );
}

/// 设备警告窗：「确认并关闭」退出。
#[test]
fn device_warning_quit_emits_quit() {
    let (mut harness, actions) = harness_warning(warning(false));
    harness.get_by_label("确认并关闭").click();
    harness.run();
    assert_eq!(
        actions.borrow().last().copied(),
        Some(OverlayWindowAction::Quit),
        "点击「确认并关闭」应发 Quit，实际：{:?}",
        actions.borrow()
    );
}

/// 勾选框：切换「不要再提示我」。
#[test]
fn device_warning_toggle_emits_suppress() {
    let (mut harness, actions) = harness_warning(warning(false));
    harness.get_by_label("不要再提示我").click();
    harness.run();
    assert_eq!(
        actions.borrow().last().copied(),
        Some(OverlayWindowAction::SuppressToggled),
        "点击勾选框应发 SuppressToggled，实际：{:?}",
        actions.borrow()
    );
}
