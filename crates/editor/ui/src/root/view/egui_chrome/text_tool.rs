//! 文字工具覆盖层：生产状态 → egui 模型 + 动作映射。

use std::sync::Arc;

use lumino_core::tr;
use lumino_message::Tool;
use lumino_ui_egui::text_tool::{TextPreview, TextToolAction, TextToolModel};

use crate::Message;
use crate::message::EditorAction;
use crate::root::Root;

/// 预览纹理缓存签名。
///
/// 任何会改变栅格结果的输入都必须计入：文字 / 模式 / 排版路径 / 全部文字参数 /
/// 字体（都在 `params` 与 `tt` 里），以及**采样几何与比例**——后者由
/// [`lumino_ui_editor::Editor::text_raster_key`] 给出（snap / ppq / k / 格数 /
/// `ss_x`,`ss_y`,`px_per_key`）。漏掉比例会让缩放后复用旧纹理。
fn preview_signature(
    editor: &lumino_ui_editor::Editor,
    tt: &lumino_editor_state::text_tool::TextToolState,
    width: u32,
    height: u32,
) -> String {
    let raster = editor.text_raster_key().unwrap_or_default();
    format!(
        "{width}x{height}:{}:{:?}:{:?}:{:?}:{raster}",
        tt.text, tt.mode, tt.layout_mode, tt.params
    )
}

impl Root {
    /// 本帧文字工具覆盖层；未激活 / 不可用时返回 `None`。
    pub fn text_tool_model(&self) -> Option<TextToolModel> {
        if !self.sidebar.piano_roll_visible {
            return None;
        }
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
                sig: preview_signature(editor, tt, w, h),
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

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::TextLayoutMode;

    #[test]
    fn test_preview_signature_tracks_params() {
        let mut editor = lumino_ui_editor::Editor::new();
        let mut state = lumino_editor_state::text_tool::TextToolState::new();
        state.set_drag(0.0, 3840.0, 60, 64);
        state.active = true;
        state.text = "A".to_string();
        editor.editor_state.text_tool = state.clone();
        let base = preview_signature(&editor, &state, 10, 20);

        state.params.size += 1.0;
        let changed = preview_signature(&editor, &state, 10, 20);
        assert_ne!(base, changed, "字号变化必须让预览重新栅格化");

        state.layout_mode = TextLayoutMode::Parametric;
        let changed_mode = preview_signature(&editor, &state, 10, 20);
        assert_ne!(changed, changed_mode, "排版路径变化必须重建预览");
    }

    /// UI-023：缩放改变 `k` ⇒ 采样比例变 ⇒ 预览签名必须变（否则复用旧纹理）。
    #[test]
    fn test_preview_signature_tracks_zoom() {
        let mut editor = lumino_ui_editor::Editor::new();
        let mut state = lumino_editor_state::text_tool::TextToolState::new();
        state.set_drag(0.0, 3840.0, 60, 64);
        state.active = true;
        state.text = "A".to_string();
        editor.editor_state.text_tool = state.clone();
        let base = preview_signature(&editor, &state, 10, 20);

        editor.editor_state.view.zoom_x *= 2.0;
        let zoomed = preview_signature(&editor, &state, 10, 20);
        assert_ne!(base, zoomed, "缩放变化必须让预览重新栅格化");

        editor.editor_state.view.snap_precision *= 2.0;
        let snapped = preview_signature(&editor, &state, 10, 20);
        assert_ne!(zoomed, snapped, "音符精度变化必须让预览重新栅格化");
    }
}
