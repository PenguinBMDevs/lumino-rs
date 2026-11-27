//! 形状抽屉画布：模型图元与输入应用。

use super::*;
use lumino_core::shape::{ShapeAnchor, Stroke, StrokeKind};
use lumino_ui_egui::shape_canvas::ShapeCanvasAction;

fn root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::ShapeDrawer,
    )
}

fn canvas_rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(256.0, 30.0), egui::vec2(744.0, 670.0))
}

#[test]
fn canvas_model_has_toolbar_grid_and_hint() {
    let root = root();
    let palette = root.egui_palette();
    let model = root.shape_canvas_model(canvas_rect(), &palette);
    assert_eq!(model.rect, canvas_rect());
    // 7 个工具 + Undo/Redo/Del。
    assert_eq!(model.buttons.len(), 10);
    assert_eq!(model.buttons[0].label, "Pen");
    assert_eq!(model.buttons[6].label, "Sel");
    assert_eq!(model.buttons[7].label, "Undo");
    assert!(!model.buttons[7].enabled, "空栈时 Undo 置灰");
    assert_eq!(model.buttons[9].label, "Del");
    // 10 条网格线 + 4 条单位框边。
    assert_eq!(model.lines.len(), 14);
    assert!(model.polys.is_empty());
    // 空文档提示 + 库路径。
    assert_eq!(model.texts.len(), 2);
    assert!(model.texts[0].text.contains("Draw a stroke"));
    assert!(model.texts[1].text.starts_with("Library:"));
}

#[test]
fn canvas_model_draws_committed_stroke() {
    let root = root();
    {
        let doc = root.state.shape_drawer.doc.clone();
        let mut doc = doc.borrow_mut();
        doc.add_stroke(Stroke {
            kind: StrokeKind::Line,
            anchors: vec![ShapeAnchor::new((0.2, 0.2)), ShapeAnchor::new((0.8, 0.8))],
            rect: None,
        });
    }
    let model = root.shape_canvas_model(canvas_rect(), &root.egui_palette());
    assert_eq!(model.polys.len(), 1, "直线描边应成为一条折线");
    assert_eq!(model.polys[0].points.len(), 2);
    assert_eq!(model.texts.len(), 1, "有描边后不再显示空提示");
    assert!(model.buttons[7].enabled, "可撤销后 Undo 点亮");
    // 新增描边默认不选中，Del 仍置灰。
    assert!(!model.buttons[9].enabled, "无选中时 Del 置灰");
}

#[test]
fn toolbar_action_switches_tool() {
    let mut root = root();
    root.apply_shape_canvas_action(&ShapeCanvasAction::Toolbar { index: 3 }, canvas_rect());
    assert_eq!(
        root.state.shape_drawer.doc.borrow().tool,
        crate::shape_drawer::doc::DrawerTool::Arc
    );
}

#[test]
fn splitter_action_sets_panel_width_with_clamp() {
    let mut root = root();
    root.apply_shape_canvas_action(&ShapeCanvasAction::Splitter(320.0), canvas_rect());
    assert!((root.state.shape_drawer.panel_width - 320.0).abs() < f32::EPSILON);
    root.apply_shape_canvas_action(&ShapeCanvasAction::Splitter(10.0), canvas_rect());
    assert!(root.state.shape_drawer.panel_width >= 160.0, "宽度有下限");
}

#[test]
fn press_and_release_commit_draft() {
    let mut root = root();
    {
        let doc = root.state.shape_drawer.doc.clone();
        let mut doc = doc.borrow_mut();
        doc.set_tool(crate::shape_drawer::doc::DrawerTool::Line);
    }
    let rect = canvas_rect();
    // 局部坐标（画布内、工具条右侧）。
    root.apply_shape_canvas_action(
        &ShapeCanvasAction::Press {
            local: egui::pos2(300.0, 200.0),
        },
        rect,
    );
    root.apply_shape_canvas_action(
        &ShapeCanvasAction::Moved {
            local: egui::pos2(420.0, 300.0),
            shift: false,
        },
        rect,
    );
    root.apply_shape_canvas_action(&ShapeCanvasAction::Release, rect);
    assert_eq!(
        root.state.shape_drawer.doc.borrow().strokes.len(),
        1,
        "按下+松开应提交一条描边"
    );
}
