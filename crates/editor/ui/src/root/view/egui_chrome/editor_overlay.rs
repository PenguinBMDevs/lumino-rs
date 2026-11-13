//! 编辑器叠加层预览：`Editor::overlay_preview()` → egui 模型（加画布偏移）。

use lumino_ui_egui::editor_overlay::EditorOverlayModel;

use crate::root::Root;

impl Root {
    /// 本帧切割线 / 饼干模刀框 / 角度柔化预览。
    pub(crate) fn editor_overlay_model(&self) -> EditorOverlayModel {
        let preview = self.editor.overlay_preview();
        let canvas = &self.editor.editor_state.canvas;
        let offset = egui::vec2(canvas.offset_x, canvas.offset_y);
        let to_pos = |p: (f32, f32)| egui::pos2(p.0, p.1) + offset;

        EditorOverlayModel {
            slice_line: preview.slice_line.map(|(a, b)| (to_pos(a), to_pos(b))),
            knife_rects: preview
                .knife_rects
                .iter()
                .map(|rect| {
                    egui::Rect::from_min_max(
                        egui::pos2(rect.x, rect.y) + offset,
                        egui::pos2(rect.x + rect.width, rect.y + rect.height) + offset,
                    )
                })
                .collect(),
            round_path: preview.round_path.into_iter().map(to_pos).collect(),
            round_handles: preview
                .round_handles
                .into_iter()
                .map(|(index, point)| (index, egui::pos2(point.x, point.y) + offset))
                .collect(),
            round_cursor_line: preview
                .round_cursor_line
                .map(|(a, b)| (to_pos(a), to_pos(b))),
            round_radius: preview.round_radius,
        }
    }
}
