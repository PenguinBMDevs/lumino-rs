//! 编辑器悬浮 √× 按钮：生产几何 → egui 模型 + 动作映射。
//!
//! 几何仍由 `lumino-ui-editor` 的 `*_button_rects` 计算（形状 / 图片转 MIDI /
//! 曲线 / 画刷共用 36×36 按钮），这里只做 canvas 局部坐标 → 窗口坐标的换算。

use lumino_message::RightSidebarAction;
use lumino_ui_editor::grid::confirm_buttons::rect_tuple;
use lumino_ui_egui::editor_action_buttons::{EditorButton, EditorButtonAction, EditorButtonsModel};

use crate::Message;
use crate::message::EditorAction;
use crate::root::Root;

impl Root {
    /// 本帧可见的悬浮 √× 按钮；没有时返回空模型。
    pub fn editor_buttons_model(&self) -> EditorButtonsModel {
        let editor = &self.editor;
        let canvas = &editor.editor_state.canvas;
        let to_window = |x: f32, y: f32, width: f32, height: f32| {
            egui::Rect::from_min_size(
                egui::pos2(x + canvas.offset_x, y + canvas.offset_y),
                egui::vec2(width, height),
            )
        };

        let mut buttons = Vec::new();
        let mut push_pair = |confirm_rect: (f32, f32, f32, f32),
                             cancel_rect: (f32, f32, f32, f32),
                             confirm,
                             cancel| {
            buttons.push(EditorButton {
                rect: to_window(
                    confirm_rect.0,
                    confirm_rect.1,
                    confirm_rect.2,
                    confirm_rect.3,
                ),
                action: confirm,
            });
            buttons.push(EditorButton {
                rect: to_window(cancel_rect.0, cancel_rect.1, cancel_rect.2, cancel_rect.3),
                action: cancel,
            });
        };

        if let Some(btns) = lumino_ui_editor::grid::shape_tool_box::shape_button_rects(editor) {
            push_pair(
                rect_tuple(btns.confirm),
                rect_tuple(btns.cancel),
                EditorButtonAction::ShapeConfirm,
                EditorButtonAction::ShapeCancel,
            );
        }
        if editor.editor_state.image_to_midi.mode == lumino_editor_state::ImageToMidiMode::Placing
            && let Some(btns) = lumino_ui_editor::grid::i2m_box::i2m_button_rects(editor)
        {
            push_pair(
                rect_tuple(btns.confirm),
                rect_tuple(btns.cancel),
                EditorButtonAction::I2mConfirm,
                EditorButtonAction::I2mCancel,
            );
        }
        if let Some(btns) = lumino_ui_editor::grid::line_tool_box::line_button_rects(editor) {
            push_pair(
                rect_tuple(btns.confirm),
                rect_tuple(btns.cancel),
                EditorButtonAction::LineConfirm,
                EditorButtonAction::LineCancel,
            );
        }
        if let Some(btns) = lumino_ui_editor::grid::brush_tool_box::brush_button_rects(editor) {
            push_pair(
                rect_tuple(btns.confirm),
                rect_tuple(btns.cancel),
                EditorButtonAction::BrushConfirm,
                EditorButtonAction::BrushCancel,
            );
        }

        let model = EditorButtonsModel { buttons };
        if !model.is_empty()
            && std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0")
        {
            tracing::info!(?model, "DEBUG editor buttons");
        }
        model
    }

    /// egui 按钮动作 → 生产消息（与 旧版 路径同源）。
    pub fn editor_button_message(&self, action: EditorButtonAction) -> Message {
        match action {
            EditorButtonAction::ShapeConfirm => {
                Message::EditorAction(EditorAction::ShapeToolConfirm)
            }
            EditorButtonAction::ShapeCancel => Message::EditorAction(EditorAction::ShapeToolCancel),
            EditorButtonAction::I2mConfirm => {
                Message::RightSidebar(RightSidebarAction::PlacementConfirm)
            }
            EditorButtonAction::I2mCancel => {
                Message::RightSidebar(RightSidebarAction::PlacementCancel)
            }
            EditorButtonAction::LineConfirm => Message::EditorAction(EditorAction::LineToolConfirm),
            EditorButtonAction::LineCancel => Message::EditorAction(EditorAction::LineToolCancel),
            EditorButtonAction::BrushConfirm => Message::EditorAction(EditorAction::BrushConfirm),
            EditorButtonAction::BrushCancel => Message::EditorAction(EditorAction::BrushCancel),
        }
    }
}
