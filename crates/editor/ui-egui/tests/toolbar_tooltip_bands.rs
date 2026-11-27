//! 工具栏悬停说明：鼠标停稳后必须自动出现「名称 + 解释」浮窗。
//!
//! 与点击用例分文件（`toolbar_controls_bands` 已接近 600 行红线）：
//! 这里只验证浮窗内容，不重复动作分发。

use egui::Pos2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{ToolbarModel, ToolbarTips};
use lumino_ui_egui::layout;
use lumino_ui_egui::widgets::HoverTip;

fn harness(model: ToolbarModel) -> Harness<'static> {
    let mut icons = lumino_ui_egui::icons::IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, layout::TOOLBAR_H))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            lumino_ui_egui::chrome::toolbar::show_model(ui, &model, &mut icons);
        });
    harness.run();
    harness
}

/// 撤销组三枚按钮的水平中心（与工具栏布局累加同源）。
fn undo_centers() -> [f32; 3] {
    let mut x = layout::TOOLBAR_PAD_X;
    x += layout::RECORD_GROUP_W + 4.0;
    x += layout::PLAYBACK_GROUP_W + 8.0;
    x += layout::LOOP_GROUP_W + 8.0;
    let start = x + (layout::UNDO_REDO_GROUP_W - layout::TOOL_BUTTON_W * 3.0) / 2.0;
    let w = layout::TOOL_BUTTON_W;
    [start + w / 2.0, start + w * 1.5, start + w * 2.5]
}

fn mid_y() -> f32 {
    layout::TOOLBAR_PAD_Y + layout::TOOLBAR_GROUP_H / 2.0
}

#[test]
fn hover_reports_explanatory_tooltip() {
    let mut h = harness(ToolbarModel {
        tips: ToolbarTips {
            undo: HoverTip::new("撤销", "撤销上一步操作"),
            ..Default::default()
        },
        ..ToolbarModel::default()
    });
    let [undo, _, _] = undo_centers();
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(Pos2::new(undo, mid_y())));
    // 悬停说明有默认延迟：多跑几帧让浮窗真正出现。
    h.run();
    h.run();
    assert!(
        h.query_by_label("撤销上一步操作").is_some(),
        "悬停撤销按钮应出现解释说明浮窗"
    );
}
