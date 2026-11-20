//! 工具栏交互测试共用的 harness 与几何正推。
//!
//! 由 `tests/common/mod.rs` 挂载；各测试二进制按需引用其中一部分。

#![allow(dead_code)] // 不同测试二进制只用到其中一部分

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::PpqBlur;
use lumino_ui_egui::chrome::toolbar_popup::{self, ToolbarAction};
use lumino_ui_egui::chrome::{ToolbarModel, toolbar as toolbar_view};
use lumino_ui_egui::layout;

/// 默认工具栏模型（PPQ 编辑缓冲初始化为 `1920`）。
pub fn model() -> ToolbarModel {
    ToolbarModel {
        ppq_buffer: "1920".to_owned(),
        ..ToolbarModel::default()
    }
}

/// 渲染工具栏并收集单槽动作。
pub fn harness(model: ToolbarModel) -> (Harness<'static>, Rc<RefCell<Vec<ToolbarAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, layout::TOOLBAR_H))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            toolbar_view::show_model(ui, &model, &mut icons);
            if let Some(action) = toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

/// 同时收集 PPQ 失焦信号（独立通道）与单槽动作的测试装置。
pub struct BlurHarness {
    /// egui 测试宿主。
    pub harness: Harness<'static>,
    /// 本帧失焦信号（`PpqBlur`）。
    pub blurs: Rc<RefCell<Vec<PpqBlur>>>,
    /// 单槽工具栏动作。
    pub actions: Rc<RefCell<Vec<ToolbarAction>>>,
}

/// 与 [`harness`] 相同，但同时收集 PPQ 输入框失焦信号。
pub fn harness_blur(model: ToolbarModel) -> BlurHarness {
    let blurs = Rc::new(RefCell::new(Vec::new()));
    let actions = Rc::new(RefCell::new(Vec::new()));
    let blurs_w = blurs.clone();
    let actions_w = actions.clone();
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, layout::TOOLBAR_H))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            toolbar_view::show_model(ui, &model, &mut icons);
            if let Some(blur) = lumino_ui_egui::chrome::toolbar::take_ppq_blur(ui.ctx()) {
                blurs_w.borrow_mut().push(blur);
            }
            if let Some(action) = toolbar_popup::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    BlurHarness {
        harness,
        blurs,
        actions,
    }
}

/// 单击（无修饰键）。
pub fn click(harness: &mut Harness<'static>, pos: Pos2) {
    click_with_modifiers(harness, pos, egui::Modifiers::default());
}

/// 带修饰键单击。
pub fn click_with_modifiers(harness: &mut Harness<'static>, pos: Pos2, modifiers: egui::Modifiers) {
    let mut input = RawInput::default();
    input.events.push(Event::ModifiersChanged(modifiers));
    input.events.push(Event::PointerMoved(pos));
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    *harness.input_mut() = input;
    harness.run();
}

/// 组内纵向中线（`group` 用 `Align::Center`）。
pub fn mid_y() -> f32 {
    layout::TOOLBAR_PAD_Y + layout::TOOLBAR_GROUP_H / 2.0
}

/// 仪表盘分组顶边（默认 72 高时 `show_model` 的 y 恰为 `TOOLBAR_PAD_Y`）。
pub fn group_top() -> f32 {
    layout::TOOLBAR_PAD_Y
}

/// 仪表盘上行（BPM / 位置）文本中心。
pub fn upper_row_y() -> f32 {
    group_top() + 15.0
}

/// 仪表盘下行（PPQ / 时间）文本中心。
pub fn lower_row_y() -> f32 {
    group_top() + 31.0
}

/// 各分组左边界（与 `show_model` 的累加完全一致）。
pub fn group_x() -> Vec<f32> {
    let mut x = layout::TOOLBAR_PAD_X;
    let record = x;
    x += layout::RECORD_GROUP_W + 4.0;
    let playback = x;
    x += layout::PLAYBACK_GROUP_W + 8.0;
    let looping = x;
    x += layout::LOOP_GROUP_W + 8.0;
    let undo = x;
    x += layout::UNDO_REDO_GROUP_W + 16.0;
    let dashboard = x;
    x += layout::DASHBOARD_GROUP_W + 16.0;
    vec![record, playback, looping, undo, dashboard, x]
}

/// 走带组三个按钮（倒带 / 播放暂停 / 快进）的横向中心。
pub fn playback_centers() -> [f32; 3] {
    let left = group_x()[1];
    let total = layout::TOOL_BUTTON_W * 3.0 + 8.0;
    let start = left + (layout::PLAYBACK_GROUP_W - total) / 2.0;
    let w = layout::TOOL_BUTTON_W;
    [
        start + w / 2.0,
        start + w + 4.0 + w / 2.0,
        start + (w + 4.0) * 2.0 + w / 2.0,
    ]
}
