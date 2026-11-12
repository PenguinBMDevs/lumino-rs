//! 文字工具覆盖层：生产状态 → egui 模型 + 动作映射。

use std::sync::Arc;

use lumino_core::tr;
use lumino_message::Tool;
use lumino_ui_egui::text_tool::{TextPreview, TextToolAction, TextToolModel};

use crate::Message;
use crate::message::EditorAction;
use crate::root::Root;

impl Root {
    /// 本帧文字工具覆盖层；未激活 / 不可用时返回 `None`。
    pub fn text_tool_model(&self) -> Option<TextToolModel> {
        let editor = &self.editor;
        if !editor.text_tool_allowed() || !editor.pending_preview_visible(Tool::Text) {
            return None;
        }
        let (left, top, right, bottom) =
            lumino_ui_editor::grid::text_tool_box::box_rect_screen(editor)?;
        let content = lumino_ui_editor::grid::utils::content_bounds(editor);
        let min_x = left.max(content.x);
        let max_x = right.min(content.x + content.width);
        let min_y = top.max(content.y);
        let max_y = bottom.min(content.y + content.height);
        if max_x <= min_x || max_y <= min_y {
            return None;
        }
        let canvas = &editor.editor_state.canvas;
        let offset = egui::vec2(canvas.offset_x, canvas.offset_y);
        let to_rect = |l: f32, t: f32, r: f32, b: f32| {
            egui::Rect::from_min_max(
                egui::pos2(l + offset.x, t + offset.y),
                egui::pos2(r + offset.x, b + offset.y),
            )
        };

        let rect = to_rect(min_x, min_y, max_x, max_y);
        let input_h = (max_y - min_y).min(28.0);
        let input_rect = to_rect(min_x, min_y, max_x, min_y + input_h);

        let mut confirm = None;
        let mut cancel = None;
        let mut mode = None;
        if editor.current_tool() == Tool::Text
            && let Some(btns) = lumino_ui_editor::grid::text_tool_box::button_rects(editor)
        {
            let rt = |t: (f32, f32, f32, f32)| to_rect(t.0, t.1, t.0 + t.2, t.1 + t.3);
            confirm = Some(rt(lumino_ui_editor::grid::confirm_buttons::rect_tuple(
                btns.confirm,
            )));
            cancel = Some(rt(lumino_ui_editor::grid::confirm_buttons::rect_tuple(
                btns.cancel,
            )));
            mode = Some(rt(lumino_ui_editor::grid::confirm_buttons::rect_tuple(
                btns.mode,
            )));
        }

        let tt = &editor.editor_state.text_tool;
        let preview = editor
            .text_tool_preview_rgba()
            .map(|(w, h, rgba)| TextPreview {
                rect: to_rect(left, top, right, bottom),
                width: w as usize,
                height: h as usize,
                rgba: Arc::new(rgba),
                sig: format!(
                    "{w}x{h}:{}:{:?}:{}",
                    tt.text, tt.mode, editor.editor_state.is_vertical_roll
                ),
            });

        let model = TextToolModel {
            rect,
            input_rect,
            text: tt.text.clone(),
            editing: tt.editing,
            placeholder: tr!("输入文字…").to_owned(),
            confirm,
            cancel,
            mode,
            mode_merged: tt.mode.is_merged(),
            preview,
        };
        if std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0") {
            tracing::info!(?model, "DEBUG text tool");
        }
        Some(model)
    }

    /// egui 文字工具动作 → 生产消息。
    pub fn text_tool_message(&self, action: TextToolAction) -> Message {
        Message::EditorAction(match action {
            TextToolAction::TextChanged(text) => EditorAction::TextToolTextChanged(text),
            TextToolAction::Confirm => EditorAction::TextToolConfirm,
            TextToolAction::Cancel => EditorAction::TextToolCancel,
            TextToolAction::ModeToggle => EditorAction::TextToolModeToggle,
        })
    }
}
