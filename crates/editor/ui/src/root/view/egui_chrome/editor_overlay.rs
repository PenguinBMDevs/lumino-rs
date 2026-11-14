//! 编辑器叠加层预览：`Editor::overlay_preview()` → egui 模型（加画布偏移）。

use lumino_core::trf;
use lumino_ui_editor::MouldKnifeInfo;
use lumino_ui_egui::editor_overlay::EditorOverlayModel;

use crate::root::Root;

impl Root {
    /// 画布覆盖层的可见裁剪：内容区 + 画布偏移，并扣掉右侧栏。
    ///
    /// 画布宽度只扣了左栏；右侧栏是覆盖在画布之上的独立面板。刀框 / 切口 /
    /// 选框等覆盖层必须裁到这里，否则纵向卷帘下会画到右侧栏上。
    pub(crate) fn canvas_overlay_clip(&self) -> Option<egui::Rect> {
        let canvas = &self.editor.editor_state.canvas;
        let local = lumino_ui_editor::grid::utils::content_bounds(&self.editor);
        if local.width <= 0.0 || local.height <= 0.0 {
            return None;
        }
        let mut rect = egui::Rect::from_min_size(
            egui::pos2(local.x + canvas.offset_x, local.y + canvas.offset_y),
            egui::vec2(local.width, local.height),
        );
        // 左侧栏宽度可调；画布偏移用的是常量宽度，覆盖层再按实际栏宽兜一层。
        let left_inset = lumino_ui_egui::layout::ROUTE_BAR_W
            + if self.sidebar.panel_visible {
                self.sidebar.panel_width
            } else {
                0.0
            };
        rect.min.x = rect.min.x.max(left_inset);
        if self.right_sidebar_visible() {
            let inset = lumino_ui_egui::layout::ROUTE_BAR_W
                + if self.right_sidebar.panel_visible {
                    self.right_sidebar.panel_width
                } else {
                    0.0
                };
            rect.max.x = (rect.max.x - inset).max(rect.min.x);
        }
        Some(rect)
    }

    /// 本帧切割线 / 饼干模刀框 / 切口 / 角度柔化预览。
    pub(crate) fn editor_overlay_model(&self) -> EditorOverlayModel {
        // 非钢琴卷帘视图（工程走带 / 瀑布流 / 导出面板）：钢琴卷帘覆盖层不绘制。
        if !self.sidebar.piano_roll_visible {
            return EditorOverlayModel::default();
        }
        let preview = self.editor.overlay_preview();
        let canvas = &self.editor.editor_state.canvas;
        let offset = egui::vec2(canvas.offset_x, canvas.offset_y);
        let to_pos = |p: (f32, f32)| egui::pos2(p.0, p.1) + offset;

        EditorOverlayModel {
            clip: self.canvas_overlay_clip(),
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
            knife_rings: preview
                .knife_rings
                .into_iter()
                .map(|ring| ring.into_iter().map(to_pos).collect())
                .collect(),
            knife_status: preview
                .knife_status
                .map(|status| (to_pos(status.anchor), knife_status_text(status.info))),
            mould_cuts: preview
                .mould_cuts
                .into_iter()
                .map(|(a, b)| (to_pos(a), to_pos(b)))
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

/// 刀状态文字（音符刀 = 条数 + 行数；几何刀 = 环数 + 行数）
fn knife_status_text(info: MouldKnifeInfo) -> String {
    match info.notes {
        Some(notes) => trf!(
            "刀：{notes} 条音符 · {rows} 行",
            notes = notes,
            rows = info.rows
        ),
        None => trf!(
            "刀：{rings} 环 · {rows} 行",
            rings = info.rings,
            rows = info.rows
        ),
    }
}
