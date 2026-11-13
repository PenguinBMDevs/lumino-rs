//! 编辑器叠加层预览几何（切割线 / 饼干模刀框 / 角度柔化）。
//!
//! 旧版把这些画在 Canvas 的 `draw` 里；egui 接管后由本模块统一暴露
//! **屏幕坐标**（不含画布左上角偏移），生产侧再转成 egui 模型绘制。

use lumino_core::shape::ShapeOp;
use lumino_ui_core::geom::{Point, Rectangle, Size};

use crate::Editor;

/// 本帧需要绘制的叠加层预览。
#[derive(Debug, Default)]
pub struct EditorOverlayPreview {
    /// 切割线：起点 / 终点（屏幕坐标）；未拖拽为 `None`。
    pub slice_line: Option<((f32, f32), (f32, f32))>,
    /// 饼干模「刀」的音符框（屏幕坐标）。
    pub knife_rects: Vec<Rectangle>,
    /// 角度柔化后的路径折线（屏幕坐标）。
    pub round_path: Vec<(f32, f32)>,
    /// 可柔化角点手柄（锚点下标 + 屏幕坐标）。
    pub round_handles: Vec<(usize, Point)>,
    /// 拖拽提示线：角点 → 当前光标（屏幕坐标）。
    pub round_cursor_line: Option<((f32, f32), (f32, f32))>,
    /// 当前拖拽角点的半径（显示 `r N.N`）。
    pub round_radius: Option<f32>,
}

impl Editor {
    /// 采集本帧全部叠加层预览（无内容时各字段为空）。
    pub fn overlay_preview(&self) -> EditorOverlayPreview {
        EditorOverlayPreview {
            slice_line: self.slice_preview_line(),
            knife_rects: self.knife_preview_rects(),
            ..self.round_corner_preview()
        }
    }

    fn slice_preview_line(&self) -> Option<((f32, f32), (f32, f32))> {
        let state = self.slice_cut.as_ref()?;
        if !state.dragging {
            return None;
        }
        let a = self.line_pos_screen_pos(state.from);
        let b = self.line_pos_screen_pos(state.to);
        Some(((a.x, a.y), (b.x, b.y)))
    }

    fn knife_preview_rects(&self) -> Vec<Rectangle> {
        let Some(walls) = self.mould_walls.as_ref() else {
            return Vec::new();
        };
        walls
            .iter()
            .map(|note| knife_note_rect(self, note))
            .collect()
    }

    fn round_corner_preview(&self) -> EditorOverlayPreview {
        let Some(mode) = self.round_corner.as_ref() else {
            return EditorOverlayPreview::default();
        };
        let Some(model) = self
            .editor_state
            .pending_composed
            .get(mode.composed)
            .and_then(|composed| composed.results.first())
            .map(|part| part.model.clone())
        else {
            return EditorOverlayPreview::default();
        };

        let anchors: Vec<(f64, f64)> = model
            .geometry
            .anchors
            .iter()
            .map(|anchor| (f64::from(anchor.pos.0), f64::from(anchor.pos.1)))
            .collect();
        let params = model.ops.iter().find_map(|op| match op {
            ShapeOp::Round(params) => Some(params),
            _ => None,
        });
        let path_points = match params {
            Some(params) => lumino_editor_state::shape_ops::rounded_points(
                &anchors,
                params,
                f64::from(params.effective_ppq(self.editor_state.view.ppq)),
                f64::from(params.effective_k(model.k)),
            ),
            None => anchors.clone(),
        };
        let round_path = path_points
            .iter()
            .map(|(tick, key)| {
                let p = self.line_pos_screen_pos((*tick as f32, *key as f32));
                (p.x, p.y)
            })
            .collect();

        let round_cursor_line = mode.corner.and_then(|corner| {
            let (tick, key) = anchors.get(corner)?;
            let cursor = mode.cursor?;
            let a = self.line_pos_screen_pos((*tick as f32, *key as f32));
            let b = self.line_pos_screen_pos((cursor.0, cursor.1));
            Some(((a.x, a.y), (b.x, b.y)))
        });
        let round_radius = mode
            .corner
            .and_then(|corner| params.and_then(|params| params.radius_of(corner)));

        EditorOverlayPreview {
            round_path,
            round_handles: self.round_corner_points(),
            round_cursor_line,
            round_radius,
            ..EditorOverlayPreview::default()
        }
    }
}

/// 单条「刀」音符的屏幕矩形（与音符绘制同口径）。
fn knife_note_rect(editor: &Editor, note: &lumino_editor_state::ShapeNote) -> Rectangle {
    let state = &editor.editor_state;
    let view = &state.view;
    if state.is_vertical_roll {
        // 纵向：tick→Y（头部在底部、向上增长），key→X
        let grid_bottom = state.canvas.size_y - view.keyboard_width;
        let x = note.key as f32 * view.zoom_y - view.scroll_y;
        let y = grid_bottom - (note.tick + note.length) * view.zoom_x + view.scroll_x;
        Rectangle::new(
            Point::new(x, y),
            Size::new(view.zoom_y, (note.length * view.zoom_x).max(1.0)),
        )
    } else {
        let x = view.tick_to_x(note.tick);
        let y = view.key_to_y(note.key);
        Rectangle::new(
            Point::new(x, y),
            Size::new((note.length * view.zoom_x).max(1.0), view.zoom_y),
        )
    }
}
