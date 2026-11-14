//! 图形叠加层（常显轮廓 / 选中高亮 / 选中框 / 编辑框 / 8+1 锚点）：生产状态 → egui 模型。
//!
//! 几何与命中同源：轮廓取 `DrawnShapeSource` 的矢量来源，选框取
//! `Editor::selection_box`，编辑框取 `Editor::shape_edit_box`，锚点取
//! `Editor::shape_edit_handles`。本文件只做画布局部坐标 → 窗口逻辑坐标的换算。

use lumino_editor_state::shape_tool::{ShapeSpec, effective_rect, shape_vertices};
use lumino_editor_state::{DrawnShape, DrawnShapeSource, ShapeKind};
use lumino_ui_egui::shape_edit::ShapeEditModel;

use crate::root::Root;

impl Root {
    /// 本帧图形叠加层（空模型 = 不显示）。
    pub fn shape_edit_model(&self) -> ShapeEditModel {
        let editor = &self.editor;
        let canvas = &editor.editor_state.canvas;
        let offset = egui::vec2(canvas.offset_x, canvas.offset_y);
        let to_rect = |rect: lumino_ui_core::geom::Rectangle| {
            egui::Rect::from_min_size(
                egui::pos2(rect.x + offset.x, rect.y + offset.y),
                egui::vec2(rect.width, rect.height),
            )
        };
        let to_pos =
            |point: lumino_ui_core::geom::Point| egui::pos2(point.x + offset.x, point.y + offset.y);

        let track = editor.editor_state.data.current_track;
        let (dtick, dkey) = editor.drag_preview_delta();
        // 鼠标工具下把当前轨未选中的可见图形以弱色常显：否则切换工具后
        // 图案只剩音符、看不见可选中范围（旧版 `drawn_shape_box` 同语义）。
        let select_mode = editor.current_tool() == lumino_message::Tool::ShapeSelect;
        let idle_outlines = if select_mode {
            editor
                .editor_state
                .shape_select
                .visible_on(track)
                .filter(|shape| !editor.editor_state.shape_select.is_selected(shape.id))
                .flat_map(|shape| outlines_of(editor, shape, 0.0, 0.0, offset))
                .collect()
        } else {
            Vec::new()
        };
        // 拖拽预览偏移：已 Bake 图形是「幽灵」语义（几何未落位），待确认镜像
        // 的几何已随拖动即时落位，再叠偏移会双倍位移。
        let selected_outlines = editor
            .editor_state
            .shape_select
            .selected_shapes_on(track)
            .flat_map(|shape| {
                let (dt, dk) = if shape.pending.is_some() {
                    (0.0, 0.0)
                } else {
                    (dtick, dkey)
                };
                outlines_of(editor, shape, dt, dk, offset)
            })
            .collect();

        // 编辑态隐藏选中选框，避免两层框叠在一起抢点击（与旧版一致）。
        let editing = editor.is_shape_edit_armed();
        let selection_box = if editing {
            None
        } else {
            editor.selection_box().map(to_rect)
        };
        // 图形框选矩形：与 `ShapeMarquee::rect` 同源（逻辑矩形 → 屏幕）。
        let marquee = editor.editor_state.shape_select.marquee().map(|marquee| {
            let (t0, t1, k0, k1) = marquee.rect();
            let a = editor.line_pos_screen_pos((t0, k0));
            let b = editor.line_pos_screen_pos((t1, k1));
            egui::Rect::from_min_max(
                egui::pos2(a.x.min(b.x) + offset.x, a.y.min(b.y) + offset.y),
                egui::pos2(a.x.max(b.x) + offset.x, a.y.max(b.y) + offset.y),
            )
        });

        let handles = editor.shape_edit_handles();
        ShapeEditModel {
            idle_outlines,
            selected_outlines,
            marquee,
            selection_box,
            edit_box: editor.shape_edit_box().map(to_rect),
            handles: handles
                .box_points
                .iter()
                .map(|point| to_pos(*point))
                .collect(),
            center: handles.center.map(to_pos),
        }
    }
}

/// 一个图形 → 若干条屏幕折线（闭合图形补首点）。
fn outlines_of(
    editor: &crate::editor::Editor,
    shape: &DrawnShape,
    dtick: f32,
    dkey: f32,
    offset: egui::Vec2,
) -> Vec<Vec<egui::Pos2>> {
    let view = &editor.editor_state.view;
    let to_pos = |(tick, key): (f32, f32)| {
        let p = editor.line_pos_screen_pos((tick + dtick, key + dkey));
        egui::pos2(p.x + offset.x, p.y + offset.y)
    };
    match &shape.source {
        DrawnShapeSource::Shape {
            kind,
            rect,
            shift_constrained,
            apex_high,
            ..
        } => {
            let spec = ShapeSpec {
                kind: *kind,
                rect: (rect.0 + dtick, rect.1 + dkey, rect.2 + dtick, rect.3 + dkey),
                shift_constrained: *shift_constrained,
                apex_high: *apex_high,
            };
            if *kind == ShapeKind::Circle {
                let (cx0, cy0, cx1, cy1) = effective_rect(spec, view.zoom_x, view.zoom_y);
                let mx = (cx0 + cx1) / 2.0;
                let my = (cy0 + cy1) / 2.0;
                let center = editor.line_pos_screen_pos((mx, my));
                let left = editor.line_pos_screen_pos((cx0, my));
                let right = editor.line_pos_screen_pos((cx1, my));
                let top = editor.line_pos_screen_pos((mx, cy1));
                let bottom = editor.line_pos_screen_pos((mx, cy0));
                let rx = (right.x - left.x).abs() * 0.5;
                let ry = (bottom.y - top.y).abs() * 0.5;
                let points = (0..=64)
                    .map(|i| {
                        let a = (i as f32 / 64.0) * std::f32::consts::TAU;
                        egui::pos2(
                            center.x + rx * a.cos() + offset.x,
                            center.y + ry * a.sin() + offset.y,
                        )
                    })
                    .collect();
                vec![points]
            } else {
                shape_vertices(spec, view.zoom_x, view.zoom_y)
                    .map(|verts| {
                        vec![closed_polyline(
                            verts.iter().map(|&point| to_pos(point)).collect(),
                        )]
                    })
                    .unwrap_or_default()
            }
        }
        DrawnShapeSource::Polyline { points } => {
            if points.is_empty() {
                Vec::new()
            } else {
                vec![points.iter().map(|&point| to_pos(point)).collect()]
            }
        }
        DrawnShapeSource::PolylineSet { polylines } => polylines
            .iter()
            .filter(|points| !points.is_empty())
            .map(|points| points.iter().map(|&point| to_pos(point)).collect())
            .collect(),
    }
}

/// 首尾不重合时补首点，保证闭合图形画满一圈。
fn closed_polyline(mut points: Vec<egui::Pos2>) -> Vec<egui::Pos2> {
    if let (Some(first), Some(last)) = (points.first().copied(), points.last().copied())
        && first != last
    {
        points.push(first);
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::storage::config::UiConfig;
    use lumino_editor_state::{DrawnShapeSource, ShapeNote};

    fn root_with_shape(pending: bool) -> Root {
        let mut root = Root::new(&UiConfig::default());
        root.editor.editor_state.data.current_track = 1;
        let id = root.editor.editor_state.shape_select.add(
            1,
            None,
            DrawnShapeSource::Polyline {
                points: vec![(0.0, 60.0), (1000.0, 64.0)],
            },
            vec![ShapeNote {
                track: 1,
                tick: 0.0,
                key: 60,
                length: 10.0,
                velocity: 100,
                channel: 0,
            }],
        );
        if pending {
            root.editor.rebuild_pending_mirrors();
            let pending_id = root
                .editor
                .editor_state
                .shape_select
                .pending_mirror(lumino_editor_state::PendingShapeRef::Shape(0))
                .expect("pending 镜像");
            root.editor
                .editor_state
                .shape_select
                .select_only(pending_id);
        } else {
            root.editor.editor_state.shape_select.select_only(id);
        }
        root
    }

    #[test]
    fn test_baked_selection_draws_box_without_handles() {
        let root = root_with_shape(false);
        let model = root.shape_edit_model();
        assert!(model.selection_box.is_some(), "已 Bake 选中应有选框");
        assert_eq!(model.selected_outlines.len(), 1, "选中折线应有一条轮廓");
        assert!(model.handles.is_empty(), "已 Bake 不应有编辑锚点");
    }

    #[test]
    fn test_baked_shape_keeps_idle_outline_when_select_tool_active() {
        let mut root = Root::new(&UiConfig::default());
        root.editor.editor_state.data.current_track = 1;
        root.editor.set_tool(lumino_message::Tool::ShapeSelect);
        root.editor.editor_state.shape_select.add(
            1,
            None,
            DrawnShapeSource::Polyline {
                points: vec![(0.0, 60.0), (1000.0, 64.0)],
            },
            Vec::new(),
        );
        let model = root.shape_edit_model();
        assert_eq!(model.idle_outlines.len(), 1, "未选中的可见图形应常显轮廓");
    }

    #[test]
    fn test_shape_marquee_is_visible_while_dragging() {
        let mut root = Root::new(&UiConfig::default());
        root.editor.editor_state.data.current_track = 1;
        root.editor.set_tool(lumino_message::Tool::ShapeSelect);
        root.editor
            .editor_state
            .shape_select
            .begin_marquee(0.0, 60.0, false);
        root.editor
            .editor_state
            .shape_select
            .update_marquee(1000.0, 64.0);

        let model = root.shape_edit_model();
        let rect = model.marquee.expect("拉框时应画框选矩形");
        assert!(rect.width() > 0.0 && rect.height() > 0.0);
    }
}
