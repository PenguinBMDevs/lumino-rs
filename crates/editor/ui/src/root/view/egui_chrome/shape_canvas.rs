//! 形状抽屉画布：文档 → egui 绘制图元，以及画布输入 → 文档变更。
//!
//! 与 旧版 `Canvas` 共用 `shape_drawer::canvas::input` 的事件逻辑与
//! `canvas::draw` 的坐标变换，保证两边行为与几何完全一致。

use egui::{Color32, Pos2};
use lumino_core::shape::{AnchorBend, AreaMap, MAP_CELLS, StrokeKind};
use lumino_ui_core::geom::{Point, Rectangle};
use lumino_ui_egui::shape_canvas::{
    ShapeCanvasAction, ShapeCanvasBezier, ShapeCanvasButton, ShapeCanvasLine, ShapeCanvasModel,
    ShapeCanvasPoly, ShapeCanvasRect, ShapeCanvasSquare, ShapeCanvasText,
};
use lumino_ui_egui::theme::Palette;

use crate::shape_drawer::canvas::draw::{
    ROW_GAP, ROW_HEIGHT, TOOLBAR_TOP, TOOLBAR_WIDTH, unit_rect, unit_to_local,
};
use crate::shape_drawer::canvas::input;
use crate::shape_drawer::doc::DrawerTool;
use crate::state::root_state::ShapeDrawerState;

/// 面积色（与 旧版 画布同一张表，alpha 0.32）。
fn area_colour(colour: u8) -> Color32 {
    const COLOURS: [(f32, f32, f32); 16] = [
        (0.0, 0.0, 0.0),
        (0.95, 0.25, 0.25),
        (0.95, 0.55, 0.20),
        (0.90, 0.80, 0.20),
        (0.35, 0.80, 0.35),
        (0.20, 0.75, 0.65),
        (0.25, 0.60, 0.95),
        (0.45, 0.40, 0.95),
        (0.75, 0.35, 0.95),
        (0.95, 0.35, 0.75),
        (0.80, 0.45, 0.30),
        (0.55, 0.70, 0.25),
        (0.20, 0.85, 0.80),
        (0.35, 0.50, 0.95),
        (0.70, 0.55, 0.90),
        (0.90, 0.60, 0.45),
    ];
    let (r, g, b) = COLOURS[usize::from(colour.min(15))];
    Color32::from_rgba_unmultiplied(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
        (0.32 * 255.0) as u8,
    )
}

/// 构建画布模型（`rect` = 画布在窗口中的矩形，局部坐标已减去其左上角）。
pub(crate) fn canvas_model(
    state: &ShapeDrawerState,
    palette: &Palette,
    rect: egui::Rect,
) -> ShapeCanvasModel {
    let doc = state.doc.borrow();
    let bounds = Rectangle::new(
        Point::new(0.0, 0.0),
        lumino_ui_core::geom::Size::new(rect.width(), rect.height()),
    );
    let text = palette.text;
    let grid = Color32::from_rgba_unmultiplied(128, 128, 128, 64);

    let mut model = ShapeCanvasModel {
        rect,
        bg: palette.base_bg,
        toolbar_bg: palette.toolbar_bg,
        toolbar_width: TOOLBAR_WIDTH,
        ..ShapeCanvasModel::default()
    };

    // 工具栏按钮。
    let tools = DrawerTool::ALL;
    for (index, tool) in tools.iter().enumerate() {
        let top = TOOLBAR_TOP + index as f32 * (ROW_HEIGHT + ROW_GAP);
        let selected = doc.tool == *tool;
        model.buttons.push(ShapeCanvasButton {
            rect: egui::Rect::from_min_size(
                egui::pos2(8.0, top),
                egui::vec2(TOOLBAR_WIDTH - 16.0, ROW_HEIGHT),
            ),
            label: tool.label().to_string(),
            fill: if selected {
                palette.accent_dark
            } else {
                palette.selected_bg
            },
            text: if selected {
                palette.text_on_weak
            } else {
                palette.text
            },
            enabled: true,
            index,
        });
    }
    let base = TOOLBAR_TOP + tools.len() as f32 * (ROW_HEIGHT + ROW_GAP) + 12.0;
    for (offset, (label, enabled)) in [
        ("Undo", !doc.undo_empty()),
        ("Redo", !doc.redo_empty()),
        ("Del", !doc.selection.is_empty()),
    ]
    .into_iter()
    .enumerate()
    {
        let top = base + offset as f32 * (ROW_HEIGHT + ROW_GAP);
        model.buttons.push(ShapeCanvasButton {
            rect: egui::Rect::from_min_size(
                egui::pos2(8.0, top),
                egui::vec2(TOOLBAR_WIDTH - 16.0, ROW_HEIGHT),
            ),
            label: label.to_string(),
            fill: if enabled {
                palette.selected_bg
            } else {
                Color32::from_rgba_unmultiplied(0, 0, 0, 31)
            },
            text: if enabled {
                palette.text
            } else {
                palette.text_on_weak
            },
            enabled,
            index: tools.len() + offset,
        });
    }

    // 网格 + 单位框。
    let unit = unit_rect(bounds, doc.view);
    for step in 0..=4 {
        let t = step as f32 / 4.0;
        let x = unit.x + unit.width * t;
        let y = unit.y + unit.height * t;
        model.lines.push(ShapeCanvasLine {
            from: Pos2::new(x, unit.y),
            to: Pos2::new(x, unit.y + unit.height),
            color: grid,
            width: 1.0,
        });
        model.lines.push(ShapeCanvasLine {
            from: Pos2::new(unit.x, y),
            to: Pos2::new(unit.x + unit.width, y),
            color: grid,
            width: 1.0,
        });
    }
    for (a, b) in [
        (
            Pos2::new(unit.x, unit.y),
            Pos2::new(unit.x + unit.width, unit.y),
        ),
        (
            Pos2::new(unit.x + unit.width, unit.y),
            Pos2::new(unit.x + unit.width, unit.y + unit.height),
        ),
        (
            Pos2::new(unit.x + unit.width, unit.y + unit.height),
            Pos2::new(unit.x, unit.y + unit.height),
        ),
        (
            Pos2::new(unit.x, unit.y + unit.height),
            Pos2::new(unit.x, unit.y),
        ),
    ] {
        model.lines.push(ShapeCanvasLine {
            from: a,
            to: b,
            color: text,
            width: 1.0,
        });
    }

    // 面积填充。
    if let Some(area) = doc.area_map_cached() {
        push_areas(&mut model, Some(area.as_ref()), unit);
    }

    // 描边 + 选中高亮。
    let selected_color = Color32::from_rgb(255, 184, 51);
    for (index, stroke) in doc.strokes.iter().enumerate() {
        let selected = doc.is_selected(index);
        push_stroke(
            &mut model,
            stroke,
            bounds,
            doc.view,
            if selected { selected_color } else { text },
            if selected { 3.0 } else { 2.0 },
        );
    }

    // 锚点（仅 Select 工具）。
    if doc.tool == DrawerTool::Select {
        let handle_color = Color32::from_rgba_unmultiplied(204, 217, 242, 217);
        for &stroke_index in &doc.selection {
            let Some(stroke) = doc.strokes.get(stroke_index) else {
                continue;
            };
            for (anchor_index, anchor) in stroke.anchors.iter().enumerate() {
                if stroke.kind == StrokeKind::Curve && anchor.bend != AnchorBend::Corner {
                    for handle in [anchor.in_handle_abs(), anchor.out_handle_abs()] {
                        if handle == anchor.pos {
                            continue;
                        }
                        let from = unit_to_local(anchor.pos, bounds, doc.view);
                        let to = unit_to_local(handle, bounds, doc.view);
                        model.lines.push(ShapeCanvasLine {
                            from: Pos2::new(from.x, from.y),
                            to: Pos2::new(to.x, to.y),
                            color: handle_color,
                            width: 1.0,
                        });
                        model.squares.push(ShapeCanvasSquare {
                            center: Pos2::new(to.x, to.y),
                            half: 2.0,
                            fill: handle_color,
                        });
                    }
                }
                let pos = unit_to_local(anchor.pos, bounds, doc.view);
                let color = if doc.selected_anchor == Some((stroke_index, anchor_index)) {
                    selected_color
                } else {
                    text
                };
                model.squares.push(ShapeCanvasSquare {
                    center: Pos2::new(pos.x, pos.y),
                    half: 3.0,
                    fill: color,
                });
            }
        }
    }

    // 提示与库路径。
    if doc.strokes.is_empty() {
        model.texts.push(ShapeCanvasText {
            pos: Pos2::new(unit.x + 16.0, unit.y + 16.0),
            text: "Draw a stroke, or pick a tool on the left".to_string(),
            size: 13.0,
            color: palette.text_on_weak,
        });
    }
    let shapes_dir = if state.shapes_dir.as_os_str().is_empty() {
        "not injected".to_string()
    } else {
        state.shapes_dir.display().to_string()
    };
    model.texts.push(ShapeCanvasText {
        pos: Pos2::new(TOOLBAR_WIDTH + 12.0, rect.height() - 24.0),
        text: format!("Library: {shapes_dir}"),
        size: 11.0,
        color: palette.text_on_weak,
    });

    // 草稿预览。
    if let Some(draft) = state.interaction.borrow().draft.as_ref()
        && let Some(stroke) = draft.to_stroke()
    {
        push_stroke(&mut model, &stroke, bounds, doc.view, text, 2.0);
    }

    model
}

/// 面积图元（同行同色合并成矩形）。
fn push_areas(model: &mut ShapeCanvasModel, map: Option<&AreaMap>, unit: Rectangle) {
    let Some(map) = map else {
        return;
    };
    let cells = usize::from(MAP_CELLS);
    let cell_w = unit.width / cells as f32;
    let cell_h = unit.height / cells as f32;
    for row in 0..cells {
        let mut col = 0;
        while col < cells {
            let colour = map.cells[row * cells + col];
            if colour == 0 || colour > 15 {
                col += 1;
                continue;
            }
            let start = col;
            col += 1;
            while col < cells && map.cells[row * cells + col] == colour {
                col += 1;
            }
            model.rects.push(ShapeCanvasRect {
                rect: egui::Rect::from_min_size(
                    egui::pos2(unit.x + start as f32 * cell_w, unit.y + row as f32 * cell_h),
                    egui::vec2((col - start) as f32 * cell_w, cell_h),
                ),
                fill: area_colour(colour),
            });
        }
    }
}

/// 单条描边（直线 / 贝塞尔 / 圆弧）。
fn push_stroke(
    model: &mut ShapeCanvasModel,
    stroke: &lumino_core::shape::Stroke,
    bounds: Rectangle,
    view: crate::shape_drawer::doc::DrawerView,
    color: Color32,
    width: f32,
) {
    if stroke.anchors.len() < 2 {
        return;
    }
    let to_pos = |p: Point| Pos2::new(p.x, p.y);
    if stroke.kind == StrokeKind::Curve {
        for window in stroke.anchors.windows(2) {
            let a = window[0];
            let b = window[1];
            if a.bend == AnchorBend::Corner || b.bend == AnchorBend::Corner {
                model.lines.push(ShapeCanvasLine {
                    from: to_pos(unit_to_local(a.pos, bounds, view)),
                    to: to_pos(unit_to_local(b.pos, bounds, view)),
                    color,
                    width,
                });
            } else {
                model.beziers.push(ShapeCanvasBezier {
                    points: [
                        to_pos(unit_to_local(a.pos, bounds, view)),
                        to_pos(unit_to_local(a.out_handle_abs(), bounds, view)),
                        to_pos(unit_to_local(b.in_handle_abs(), bounds, view)),
                        to_pos(unit_to_local(b.pos, bounds, view)),
                    ],
                    color,
                    width,
                });
            }
        }
        return;
    }
    if stroke.kind == StrokeKind::Arc && stroke.anchors.len() >= 3 {
        let a = stroke.anchors[0].pos;
        let b = stroke.anchors[1].pos;
        let c = stroke.anchors[2].pos;
        let points = lumino_editor_state::shape_ops::arc_points_scaled(
            (a.0 as f64, a.1 as f64),
            (b.0 as f64, b.1 as f64),
            (c.0 as f64, c.1 as f64),
            1.0,
        );
        model.polys.push(ShapeCanvasPoly {
            points: points
                .iter()
                .map(|p| to_pos(unit_to_local((p.0 as f32, p.1 as f32), bounds, view)))
                .collect(),
            color,
            width,
        });
        return;
    }
    model.polys.push(ShapeCanvasPoly {
        points: stroke
            .anchors
            .iter()
            .map(|anchor| to_pos(unit_to_local(anchor.pos, bounds, view)))
            .collect(),
        color,
        width,
    });
}

impl crate::root::Root {
    /// 画布模型（供叠加层调用）。
    pub(crate) fn shape_canvas_model(
        &self,
        rect: egui::Rect,
        palette: &Palette,
    ) -> ShapeCanvasModel {
        canvas_model(&self.state.shape_drawer, palette, rect)
    }

    /// 把 egui 画布动作应用到文档（与 旧版 `Canvas::update` 同一套逻辑）。
    pub(crate) fn apply_shape_canvas_action(
        &mut self,
        action: &ShapeCanvasAction,
        rect: egui::Rect,
    ) {
        let state = &self.state.shape_drawer;
        let doc = state.doc.clone();
        let interaction = state.interaction.clone();
        let bounds = Rectangle::new(
            Point::new(0.0, 0.0),
            lumino_ui_core::geom::Size::new(rect.width(), rect.height()),
        );
        match action {
            ShapeCanvasAction::Press { local } => {
                input::press(&doc, &interaction, bounds, Point::new(local.x, local.y));
            }
            ShapeCanvasAction::Moved { local, shift } => {
                input::moved(
                    &doc,
                    &interaction,
                    bounds,
                    Point::new(local.x, local.y),
                    *shift,
                );
            }
            ShapeCanvasAction::Release => input::release(&doc, &interaction),
            ShapeCanvasAction::MiddlePress { local } => {
                input::middle_press(&interaction, Point::new(local.x, local.y));
            }
            ShapeCanvasAction::MiddleRelease => input::middle_release(&interaction),
            ShapeCanvasAction::Scroll { lines, up } => input::scroll(&doc, *lines, *up),
            ShapeCanvasAction::Toolbar { index } => {
                input::toolbar_index(&doc, &interaction, *index)
            }
            ShapeCanvasAction::Delete => input::delete(&doc),
            ShapeCanvasAction::Undo => input::undo(&doc),
            ShapeCanvasAction::Redo => input::redo(&doc),
            ShapeCanvasAction::Splitter(x) => {
                self.state.shape_drawer.panel_width = x.clamp(160.0, 420.0);
            }
        }
    }
}
