//! 编辑器叠加层预览几何（切割线 / 饼干模刀框 / 角度柔化）。
//!
//! 旧版把这些画在 Canvas 的 `draw` 里；egui 接管后由本模块统一暴露
//! **屏幕坐标**（不含画布左上角偏移），生产侧再转成 egui 模型绘制。

use lumino_core::shape::ShapeOp;
use lumino_ui_core::geom::{Point, Rectangle, Size};

use crate::Editor;
use crate::interaction::shape_compose::MouldKnifeInfo;

/// 本帧需要绘制的叠加层预览。
#[derive(Debug, Default)]
pub struct EditorOverlayPreview {
    /// 切割线：起点 / 终点（屏幕坐标）；未拖拽为 `None`。
    pub slice_line: Option<((f32, f32), (f32, f32))>,
    /// 饼干模「刀」的音符框（屏幕坐标）。
    pub knife_rects: Vec<Rectangle>,
    /// 饼干模「刀」的几何环（屏幕坐标；几何刀）。
    pub knife_rings: Vec<Vec<(f32, f32)>>,
    /// 刀状态文字（锚点 + 摘要）；无刀或非鼠标工具为 `None`。
    pub knife_status: Option<KnifeStatus>,
    /// 饼干模切口（屏幕坐标线段）；刀清空后仍保留到 ✓ / ×。
    pub mould_cuts: Vec<((f32, f32), (f32, f32))>,
    /// 角度柔化后的路径折线（屏幕坐标）。
    pub round_path: Vec<(f32, f32)>,
    /// 可柔化角点手柄（锚点下标 + 屏幕坐标）。
    pub round_handles: Vec<(usize, Point)>,
    /// 拖拽提示线：角点 → 当前光标（屏幕坐标）。
    pub round_cursor_line: Option<((f32, f32), (f32, f32))>,
    /// 当前拖拽角点的半径（显示 `r N.N`）。
    pub round_radius: Option<f32>,
}

/// 刀状态（覆盖层文字）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KnifeStatus {
    /// 文字锚点（屏幕坐标，左下对齐）
    pub anchor: (f32, f32),
    /// 条数 / 行数 / 环数
    pub info: MouldKnifeInfo,
}

/// 刀预览的三段产物：音符框 / 几何环 / 状态文字
type KnifePreview = (Vec<Rectangle>, Vec<Vec<(f32, f32)>>, Option<KnifeStatus>);

impl Editor {
    /// 采集本帧全部叠加层预览（无内容时各字段为空）。
    pub fn overlay_preview(&self) -> EditorOverlayPreview {
        let (knife_rects, knife_rings, knife_status) = self.knife_preview();
        EditorOverlayPreview {
            slice_line: self.slice_preview_line(),
            knife_rects,
            knife_rings,
            knife_status,
            mould_cuts: self.mould_cut_lines(),
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

    /// 刀框 / 刀环 / 状态文字
    ///
    /// 刀是显式会话，任何工具下都可见（设刀后立刻能看到，而不是切到鼠标工具才出现）。
    /// 几何刀的源被删/改时立即隐藏；音符刀由 `set_tool` / 删除路径的
    /// `sync_mould_knife` 收口——避免每帧全轨扫描。
    fn knife_preview(&self) -> KnifePreview {
        let Some(knife) = self.mould_knife.as_ref() else {
            return (Vec::new(), Vec::new(), None);
        };
        if knife.is_geometry_stale(self) {
            return (Vec::new(), Vec::new(), None);
        }
        let rects: Vec<Rectangle> = knife
            .notes()
            .iter()
            .map(|note| knife_note_rect(self, note))
            .collect();
        let rings: Vec<Vec<(f32, f32)>> = knife
            .rings()
            .iter()
            .map(|ring| {
                ring.iter()
                    .map(|(tick, key)| {
                        let point = self.line_pos_screen_pos((*tick, *key));
                        (point.x, point.y)
                    })
                    .collect()
            })
            .collect();
        let status = knife_status_anchor(&rects, &rings).map(|anchor| KnifeStatus {
            anchor,
            info: knife.info(),
        });
        (rects, rings, status)
    }

    /// 饼干模切口（空隙区间 → 屏幕线段）
    fn mould_cut_lines(&self) -> Vec<((f32, f32), (f32, f32))> {
        if self.current_tool() != lumino_message::Tool::ShapeSelect {
            return Vec::new();
        }
        let mut out = Vec::new();
        for composed in &self.editor_state.pending_composed {
            if composed.kind != lumino_editor_state::ComposedKind::Mould {
                continue;
            }
            for &(key, start, end) in &composed.mould_cuts {
                out.push(mould_cut_segment(self, key, start, end));
            }
        }
        out
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

/// 刀状态文字的锚点：刀框 / 刀环的左上角再上移一点；无几何时 `None`
fn knife_status_anchor(rects: &[Rectangle], rings: &[Vec<(f32, f32)>]) -> Option<(f32, f32)> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    for rect in rects {
        min_x = min_x.min(rect.x);
        min_y = min_y.min(rect.y);
    }
    for &(x, y) in rings.iter().flatten() {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
    }
    (min_x.is_finite() && min_y.is_finite()).then_some((min_x, min_y - 4.0))
}

/// 切口：key 行的区间 → 屏幕线段（横排取行中线，纵排取列中线）
fn mould_cut_segment(editor: &Editor, key: u16, start: f32, end: f32) -> ((f32, f32), (f32, f32)) {
    let state = &editor.editor_state;
    let view = &state.view;
    if state.is_vertical_roll {
        let grid_bottom = state.canvas.size_y - view.keyboard_width;
        let x = key as f32 * view.zoom_y - view.scroll_y + view.zoom_y * 0.5;
        let y0 = grid_bottom - start * view.zoom_x + view.scroll_x;
        let y1 = grid_bottom - end * view.zoom_x + view.scroll_x;
        ((x, y0), (x, y1))
    } else {
        let y = view.key_to_y(key) + view.zoom_y * 0.5;
        ((view.tick_to_x(start), y), (view.tick_to_x(end), y))
    }
}
