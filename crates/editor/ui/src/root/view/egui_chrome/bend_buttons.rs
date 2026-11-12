//! 曲线弯折按钮：生产几何 → egui 模型 + 动作映射。

use lumino_core::tr;
use lumino_ui_egui::bend_buttons::{BendButton, BendButtonsModel, BendKind};

use crate::Message;
use crate::message::EditorAction;
use crate::root::Root;

impl Root {
    /// 本帧曲线弯折按钮（选中锚点存在时显示）。
    pub fn bend_buttons_model(&self) -> BendButtonsModel {
        let editor = &self.editor;
        if std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0") {
            tracing::info!(
                tool = ?editor.current_tool(),
                selected = ?editor.editor_state.line_tool.selected_anchor(),
                paths = ?editor
                    .editor_state
                    .line_tool
                    .paths
                    .iter()
                    .map(|path| path.len())
                    .collect::<Vec<_>>(),
                "DEBUG bend state"
            );
        }
        let Some(btns) = lumino_ui_editor::grid::line_tool_box::line_bend_button_rects(editor)
        else {
            return BendButtonsModel::default();
        };
        let canvas = &editor.editor_state.canvas;
        let to_window = |x: f32, y: f32, width: f32, height: f32| {
            egui::Rect::from_min_size(
                egui::pos2(x + canvas.offset_x, y + canvas.offset_y),
                egui::vec2(width, height),
            )
        };
        let current = btns.current;
        let (sx, sy, sw, sh) = lumino_ui_editor::grid::confirm_buttons::rect_tuple(btns.smooth);
        let (cx, cy, cw, ch) = lumino_ui_editor::grid::confirm_buttons::rect_tuple(btns.corner);
        let model = BendButtonsModel {
            buttons: vec![
                BendButton {
                    rect: to_window(sx, sy, sw, sh),
                    label: tr!("曲线").to_owned(),
                    kind: BendKind::Smooth,
                    selected: current == lumino_editor_state::AnchorBend::Smooth,
                },
                BendButton {
                    rect: to_window(cx, cy, cw, ch),
                    label: tr!("折线").to_owned(),
                    kind: BendKind::Corner,
                    selected: current == lumino_editor_state::AnchorBend::Corner,
                },
            ],
        };
        if std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0") {
            tracing::info!(?model, "DEBUG bend buttons");
        }
        model
    }

    /// egui 弯折动作 → 生产消息。
    pub fn bend_button_message(&self, bend: lumino_editor_state::AnchorBend) -> Message {
        Message::EditorAction(EditorAction::LineToolBend(bend))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::shape::ShapeAnchor;
    use lumino_core::storage::config::UiConfig;
    use lumino_editor_state::LineToolInteraction;
    use lumino_ui_egui::bend_buttons::BendKind;

    /// 选中中间锚点时：模型给出两枚按钮，动作映射走 `LineToolBend`。
    #[test]
    fn middle_anchor_shows_two_bend_buttons() {
        let mut root = Root::new(&UiConfig::default());
        root.editor.editor_state.data.current_track = 1;
        root.editor.editor_state.canvas.size_x = 1280.0;
        root.editor.editor_state.canvas.size_y = 800.0;
        root.editor.editor_state.tool = lumino_message::Tool::Curve;
        root.editor.editor_state.line_tool.paths.push(vec![
            ShapeAnchor::new((1920.0, 60.0)),
            ShapeAnchor::new((3840.0, 62.0)),
            ShapeAnchor::new((5760.0, 60.0)),
        ]);
        root.editor.editor_state.line_tool.interaction =
            LineToolInteraction::SelectedAnchor { path: 0, idx: 1 };

        let model = root.bend_buttons_model();
        assert_eq!(model.buttons.len(), 2, "中间锚点应有曲线/折线两枚按钮");
        assert!(model.buttons.iter().any(|b| b.kind == BendKind::Smooth));
        assert!(model.buttons.iter().any(|b| b.kind == BendKind::Corner));

        let message = root.bend_button_message(lumino_editor_state::AnchorBend::Corner);
        assert!(matches!(
            message,
            Message::EditorAction(EditorAction::LineToolBend(
                lumino_editor_state::AnchorBend::Corner
            ))
        ));
    }

    /// 端点锚点（idx 0 / 末位）不显示弯折按钮。
    #[test]
    fn endpoint_anchor_has_no_buttons() {
        let mut root = Root::new(&UiConfig::default());
        root.editor.editor_state.data.current_track = 1;
        root.editor.editor_state.canvas.size_x = 1280.0;
        root.editor.editor_state.canvas.size_y = 800.0;
        root.editor.editor_state.tool = lumino_message::Tool::Curve;
        root.editor.editor_state.line_tool.paths.push(vec![
            ShapeAnchor::new((1920.0, 60.0)),
            ShapeAnchor::new((3840.0, 62.0)),
            ShapeAnchor::new((5760.0, 60.0)),
        ]);
        root.editor.editor_state.line_tool.interaction =
            LineToolInteraction::SelectedAnchor { path: 0, idx: 0 };
        assert!(root.bend_buttons_model().is_empty());
    }
}
