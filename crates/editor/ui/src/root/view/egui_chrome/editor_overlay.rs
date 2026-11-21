//! 编辑器叠加层预览：`Editor::overlay_preview()` → egui 模型（加画布偏移）。

use lumino_core::trf;
use lumino_ui_editor::MouldKnifeInfo;
use lumino_ui_egui::editor_overlay::EditorOverlayModel;

use crate::root::Root;

impl Root {
    /// 画布覆盖层的可见裁剪：内容区 + 画布偏移。
    ///
    /// `canvas.size_x` 由 `Host::canvas_geometry` 写入时已扣掉左栏、右侧栏（含
    /// 图标列与展开面板）和滚动条，是画布可见宽度的唯一权威。本函数只把本地
    /// 内容区平移到屏幕坐标，**不得再扣一次右侧栏**——否则右侧栏展开时选框 /
    /// 刀线会在右侧被裁窄一个侧栏宽度（DEBT-17 引入的重复扣减）。
    /// 左边界保留幂等钳制：侧栏宽度变化而画布几何尚未回写的那一帧，覆盖层
    /// 不能画进路由栏 / 左侧栏。
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
        let left_inset = lumino_ui_egui::layout::ROUTE_BAR_W
            + if self.sidebar.panel_visible {
                self.sidebar.panel_width
            } else {
                0.0
            };
        rect.min.x = rect.min.x.max(left_inset);
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
