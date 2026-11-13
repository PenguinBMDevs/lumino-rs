//! 曲线预览：生产几何 → egui 模型。
//!
//! 几何与 旧版 `line_tool_box::draw` 同源：路径锚点 / 弯折 / 控制柄 / 产物块
//! 全部直接取 `line_tool` 状态与 `path_outline_blocks`，只做坐标换算。

use lumino_editor_state::HandleSide;
use lumino_message::Tool;
use lumino_ui_egui::curve_preview::{AnchorMark, CurvePreviewModel, CurveSegment, HANDLE_SIZE};

use crate::root::Root;

impl Root {
    /// 本帧曲线工具预览。
    pub fn curve_preview_model(&self) -> CurvePreviewModel {
        let editor = &self.editor;
        if editor.editor_state.data.current_track == 0
            || !editor.pending_preview_visible(Tool::Curve)
        {
            return CurvePreviewModel::default();
        }
        let line = &editor.editor_state.line_tool;
        if line.paths.is_empty() {
            return CurvePreviewModel::default();
        }

        let canvas = &editor.editor_state.canvas;
        let offset = egui::vec2(canvas.offset_x, canvas.offset_y);
        let to_point = |pos: (f32, f32)| {
            let p = editor.line_pos_screen_pos(pos);
            egui::pos2(p.x + offset.x, p.y + offset.y)
        };
        let to_rect = |t0: f32, t1: f32, key: f32| {
            let a = editor.line_pos_screen_pos((t0, key - 0.5));
            let b = editor.line_pos_screen_pos((t1, key + 0.5));
            egui::Rect::from_min_max(
                egui::pos2(a.x.min(b.x) + offset.x, a.y.min(b.y) + offset.y),
                egui::pos2(a.x.max(b.x) + offset.x, a.y.max(b.y) + offset.y),
            )
        };

        let selected = line.selected_anchor();
        let show_handles = editor.current_tool() == Tool::Curve;
        let mut segments = Vec::new();
        let mut blocks = Vec::new();
        let mut connectors = Vec::new();
        let mut handles = Vec::new();
        let mut anchors = Vec::new();
        let mut fill_blocks = Vec::new();
        let mut fill_loops = Vec::new();

        if let Some(fill) = lumino_ui_editor::grid::line_tool_box::fill_preview(editor) {
            for (t0, t1, key) in fill.blocks {
                fill_blocks.push(to_rect(t0, t1, key));
            }
            for lp in fill.loops {
                fill_loops.push(
                    lp.iter()
                        .map(|&(tick, key)| to_point((tick, key)))
                        .collect(),
                );
            }
        }

        for (pi, path) in line.paths.iter().enumerate() {
            // 圆弧模式：与 √ 产物同源的采样折线（圆心在锚点外，不能画贝塞尔）。
            if line.arc_mode {
                if let Some(points) = editor.line_arc_points(path) {
                    for pair in points.windows(2) {
                        segments.push(CurveSegment::Line(to_point(pair[0]), to_point(pair[1])));
                    }
                }
            } else {
                for pair in path.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    let pa = to_point(a.pos);
                    let pb = to_point(b.pos);
                    if a.bend == lumino_editor_state::AnchorBend::Corner
                        || b.bend == lumino_editor_state::AnchorBend::Corner
                    {
                        segments.push(CurveSegment::Line(pa, pb));
                    } else {
                        segments.push(CurveSegment::Cubic(
                            pa,
                            to_point(a.out_handle_abs()),
                            to_point(b.in_handle_abs()),
                            pb,
                        ));
                    }
                }
            }

            for (t0, t1, key) in editor.path_outline_blocks(path) {
                blocks.push(to_rect(t0, t1, f32::from(key)));
            }

            if !show_handles {
                continue;
            }
            for (ai, anchor) in path.iter().enumerate() {
                let ap = to_point(anchor.pos);
                // 圆弧模式没有可拖控制柄（与命中分发一致）。
                if !line.arc_mode {
                    for side in line.visible_handle_sides(pi, ai) {
                        let hp = to_point(match side {
                            HandleSide::In => anchor.in_handle_abs(),
                            HandleSide::Out => anchor.out_handle_abs(),
                        });
                        connectors.push((ap, hp));
                        handles.push(egui::Rect::from_center_size(
                            hp,
                            egui::vec2(HANDLE_SIZE, HANDLE_SIZE),
                        ));
                    }
                }
                anchors.push(AnchorMark {
                    pos: ap,
                    selected: selected == Some((pi, ai)),
                });
            }
        }

        let model = CurvePreviewModel {
            segments,
            blocks,
            connectors,
            handles,
            anchors,
            fill_blocks,
            fill_loops,
        };
        if std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0") {
            let points: Vec<(f32, f32)> =
                model.anchors.iter().map(|a| (a.pos.x, a.pos.y)).collect();
            tracing::info!(?points, "DEBUG curve anchors");
        }
        model
    }
}
