//! 形状工具预览：生产几何 → egui 模型。
//!
//! 几何与 旧版 `shape_tool_box::draw_one_shape` 同源：
//! - `effective_rect` + `shape_vertices`（圆用 64 段多边形逼近）；
//! - `shape_outline_blocks`（与 √ 写入同源）逐块转矩形。

use lumino_editor_state::ShapeKind;
use lumino_editor_state::shape_tool::{ShapeSpec, effective_rect, shape_vertices};
use lumino_ui_egui::shape_preview::{PreviewShape, ShapePreviewModel};

use crate::root::Root;

impl Root {
    /// 本帧形状工具预览（待确认图形 + 实时拖拽）。
    pub fn shape_preview_model(&self) -> ShapePreviewModel {
        let editor = &self.editor;
        if editor.editor_state.data.current_track == 0
            || !editor.pending_preview_visible(lumino_message::Tool::Shape)
        {
            return ShapePreviewModel::default();
        }
        let tool = &editor.editor_state.shape_tool;
        if !tool.has_pending()
            && !tool.is_dragging()
            && editor.editor_state.pending_composed.is_empty()
        {
            return ShapePreviewModel::default();
        }

        let canvas = &editor.editor_state.canvas;
        let offset = egui::vec2(canvas.offset_x, canvas.offset_y);
        let mut shapes = Vec::new();

        for shape in &tool.shapes {
            shapes.push(preview_shape(editor, shape.spec(), shape.filled, offset));
        }
        // 拖拽预览只属于形状工具本身：切到鼠标工具后即使拖拽态晚一帧清空，
        // 也不能在画布上多画一个「角点」（表现为切换工具时闪一下）。
        if editor.current_tool() == lumino_message::Tool::Shape
            && let Some((spec, filled)) = tool.preview_rect(editor.shift_pressed())
        {
            shapes.push(preview_shape(editor, spec, filled, offset));
        }

        // 待确认组合（DRAW-020/027/028/DEBT-14）：直接画 ✓ 会写入的音符块，
        // 否则「插入」只看到几何轮廓，无法判断最终产物。
        for composed in &editor.editor_state.pending_composed {
            let mut blocks = Vec::new();
            for note in composed.notes() {
                let t0 = note.tick;
                let t1 = (note.tick + note.length).max(note.tick + 1.0);
                let k = f32::from(note.key);
                let a = editor.line_pos_screen_pos((t0, k - 0.5));
                let b = editor.line_pos_screen_pos((t1, k + 0.5));
                blocks.push(egui::Rect::from_min_max(
                    egui::pos2(a.x.min(b.x) + offset.x, a.y.min(b.y) + offset.y),
                    egui::pos2(a.x.max(b.x) + offset.x, a.y.max(b.y) + offset.y),
                ));
            }
            if !blocks.is_empty() {
                shapes.push(PreviewShape {
                    points: Vec::new(),
                    filled: false,
                    blocks,
                });
            }
        }

        if std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0") {
            for shape in &shapes {
                tracing::info!(?shape.points, ?shape.blocks, "DEBUG shape preview");
            }
        }
        ShapePreviewModel { shapes }
    }
}

fn preview_shape(
    editor: &crate::editor::Editor,
    spec: ShapeSpec,
    filled: bool,
    offset: egui::Vec2,
) -> PreviewShape {
    let view = &editor.editor_state.view;
    let (px_per_tick, px_per_key) = (view.zoom_x, view.zoom_y);
    let rect = effective_rect(spec, px_per_tick, px_per_key);
    let points: Vec<egui::Pos2> = match spec.kind {
        ShapeKind::Circle => {
            let (cx0, cy0, cx1, cy1) = rect;
            let mx = (cx0 + cx1) / 2.0;
            let my = (cy0 + cy1) / 2.0;
            let center = editor.line_pos_screen_pos((mx, my));
            let left = editor.line_pos_screen_pos((cx0, my));
            let right = editor.line_pos_screen_pos((cx1, my));
            let top = editor.line_pos_screen_pos((mx, cy1));
            let bottom = editor.line_pos_screen_pos((mx, cy0));
            let rx = (right.x - left.x).abs() * 0.5;
            let ry = (bottom.y - top.y).abs() * 0.5;
            (0..64)
                .map(|i| {
                    let a = (i as f32 / 64.0) * std::f32::consts::TAU;
                    egui::pos2(
                        center.x + rx * a.cos() + offset.x,
                        center.y + ry * a.sin() + offset.y,
                    )
                })
                .collect()
        }
        _ => shape_vertices(spec, px_per_tick, px_per_key)
            .map(|verts| {
                verts
                    .iter()
                    .map(|&(tick, key)| {
                        let p = editor.line_pos_screen_pos((tick, key));
                        egui::pos2(p.x + offset.x, p.y + offset.y)
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };

    let blocks = editor
        .shape_outline_blocks(spec, filled)
        .into_iter()
        .map(|(t0, t1, key)| {
            let k = f32::from(key);
            let a = editor.line_pos_screen_pos((t0, k - 0.5));
            let b = editor.line_pos_screen_pos((t1, k + 0.5));
            egui::Rect::from_min_max(
                egui::pos2(a.x.min(b.x) + offset.x, a.y.min(b.y) + offset.y),
                egui::pos2(a.x.max(b.x) + offset.x, a.y.max(b.y) + offset.y),
            )
        })
        .collect();

    PreviewShape {
        points,
        filled,
        blocks,
    }
}
