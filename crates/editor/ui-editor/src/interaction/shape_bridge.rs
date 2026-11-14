//! `DrawnShape` ↔ `ShapeModel` bridge（CP-4.5a）
//!
//! UI 层原有 `DrawnShapeSource` 负责渲染/命中；019–022 的算法吃 `ShapeModel`。
//! 这里只做最小桥接：几何转换 + 保留原音符，不重新生成。

use lumino_core::shape::{ShapeAnchor, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp, ShapeStyle};
use lumino_editor_state::shape_ops::copy_along_copies_of_points;
use lumino_editor_state::shape_ops::symmetric_shape;
use lumino_editor_state::shape_tool::shape_outline_path;
use lumino_editor_state::{DrawnShape, DrawnShapeSource, ShapeKind as UiShapeKind, ShapeSpec};

use crate::Editor;

/// 选中图形 → ShapeModel（仅几何；音符由调用方保留）
pub(crate) fn model_from_drawn(
    drawn: &DrawnShape,
    k: f32,
    px_per_tick: f32,
    px_per_key: f32,
) -> ShapeModel {
    model_from_source(&drawn.source, drawn.track, k, px_per_tick, px_per_key)
}

/// 几何来源 → ShapeModel（Bake 保留模型 / 选中图形共用）
///
/// `px_per_tick` / `px_per_key` 必须传**当前视图**的缩放：Shift 约束的
/// 正图形（圆/正方形/等边三角形）是在屏幕空间求解的，传 1.0 会把几何
/// 压扁（圆的下半被截断、正方形变成细长条）。
pub(crate) fn model_from_source(
    source: &DrawnShapeSource,
    track: usize,
    k: f32,
    px_per_tick: f32,
    px_per_key: f32,
) -> ShapeModel {
    let mut model = ShapeModel::new(0, track as u16, ShapeKind::Custom, k);
    model.style = ShapeStyle::default();
    match source {
        DrawnShapeSource::Polyline { points } => {
            model.kind = ShapeKind::Polyline;
            model.geometry = ShapeGeometry::from_points(points.iter().copied());
            let closed = points.len() >= 3
                && points.first().is_some_and(|first| {
                    points.last().is_some_and(|last| {
                        (first.0 - last.0).abs() < 1e-3 && (first.1 - last.1).abs() < 1e-3
                    })
                });
            model.geometry.closed = closed;
        }
        DrawnShapeSource::Shape {
            kind,
            rect,
            shift_constrained,
            apex_high,
            filled,
        } => {
            let spec = ShapeSpec {
                kind: *kind,
                rect: *rect,
                shift_constrained: *shift_constrained,
                apex_high: *apex_high,
            };
            let points = shape_outline_path(spec, px_per_tick, px_per_key);
            model.kind = ShapeKind::Custom;
            model.geometry = ShapeGeometry::from_points(points);
            // 形状工具产物都是闭合图形；保留模型沿用原始填充语义，
            // 载入后 Outline 模型可用同源轮廓音符接管，032B 编辑时再切 Fill。
            model.geometry.closed = true;
            if *filled {
                model.style.fill = lumino_core::shape::ShapeFill::Fill;
            }
        }
        DrawnShapeSource::PolylineSet { polylines } => {
            // 仅用于把组合预览还原成可分析模型（不会再生成音符）；多段拍平为一条折线。
            let points: Vec<(f32, f32)> = polylines.iter().flatten().copied().collect();
            model.kind = ShapeKind::Polyline;
            model.geometry = ShapeGeometry::from_points(points);
            model.geometry.closed = false;
        }
    }
    model
}

/// 模型的可渲染折线集合：Live 优先用各段 stroke，其它用整条几何；
/// CopyAlong 额外展开副本折线（预览 + 镜像命中都覆盖副本）。
pub(crate) fn polylines_of_model(model: &ShapeModel, ppq: f32) -> Vec<Vec<(f32, f32)>> {
    let base = if let Some(ShapeOp::Live(params)) =
        model.ops.iter().find(|op| matches!(op, ShapeOp::Live(_)))
    {
        let strokes: Vec<Vec<(f32, f32)>> = params
            .strokes
            .iter()
            .filter(|stroke| !stroke.anchors.is_empty())
            .map(|stroke| stroke.anchors.iter().map(|anchor| anchor.pos).collect())
            .collect();
        if !strokes.is_empty() {
            strokes
        } else {
            geometry_polylines(&model.geometry.anchors)
        }
    } else {
        // 023 半边对称在生成阶段展开锚点；预览/命中必须用同一份展开结果，
        // 否则轮廓只画半边，看起来像「对称被截断」。
        geometry_polylines(&render_anchors(model))
    };
    let Some(ShapeOp::CopyAlong(params)) = model
        .ops
        .iter()
        .find(|op| matches!(op, ShapeOp::CopyAlong(_)))
    else {
        return base;
    };
    let mut out = base.clone();
    for polyline in &base {
        out.extend(copy_along_copies_of_points(
            polyline,
            params,
            f64::from(ppq),
        ));
    }
    out
}

/// 渲染/命中用锚点：应用生成阶段的对称展开（023）。
fn render_anchors(model: &ShapeModel) -> Vec<ShapeAnchor> {
    if let Some(params) = model.ops.iter().find_map(|op| match op {
        ShapeOp::Symmetry(params) => Some(params),
        _ => None,
    }) && let Some(expanded) = symmetric_shape(model, params)
    {
        return expanded;
    }
    model.geometry.anchors.clone()
}

fn geometry_polylines(anchors: &[ShapeAnchor]) -> Vec<Vec<(f32, f32)>> {
    if anchors.is_empty() {
        Vec::new()
    } else {
        vec![anchors.iter().map(|anchor| anchor.pos).collect()]
    }
}

/// ShapeModel → 可渲染的 DrawnShapeSource
///
/// DEC-001 后生产路径不再用形状回写预览（已 Bake 只读）；保留给只读展示与测试。
#[allow(dead_code)]
pub(crate) fn source_from_model(model: &ShapeModel) -> DrawnShapeSource {
    if !model.geometry.anchors.is_empty() {
        return DrawnShapeSource::Polyline {
            points: model
                .geometry
                .anchors
                .iter()
                .map(|anchor| anchor.pos)
                .collect(),
        };
    }
    if let Some(rect) = model.geometry.rect {
        return DrawnShapeSource::Shape {
            kind: UiShapeKind::Rectangle,
            rect: (rect.tick_lo, rect.key_lo, rect.tick_hi, rect.key_hi),
            shift_constrained: false,
            filled: false,
            apex_high: true,
        };
    }
    DrawnShapeSource::Polyline { points: Vec::new() }
}

impl Editor {
    /// 形状几何的屏幕尺度（px per tick / px per key），与预览/√ 同一口径
    pub(crate) fn shape_px_per_unit(&self) -> (f32, f32) {
        (self.editor_state.view.zoom_x, self.editor_state.view.zoom_y)
    }

    /// 用当前视图尺度把几何来源转成模型（Shift 约束图形必须走这里）
    pub(crate) fn model_from_source_at_view(
        &self,
        source: &DrawnShapeSource,
        track: usize,
        k: f32,
    ) -> ShapeModel {
        let (px_per_tick, px_per_key) = self.shape_px_per_unit();
        model_from_source(source, track, k, px_per_tick, px_per_key)
    }

    /// 用当前视图尺度把已绘制图形转成模型
    pub(crate) fn model_from_drawn_at_view(&self, drawn: &DrawnShape, k: f32) -> ShapeModel {
        let (px_per_tick, px_per_key) = self.shape_px_per_unit();
        model_from_drawn(drawn, k, px_per_tick, px_per_key)
    }
}

#[cfg(test)]
mod tests;
