//! `DrawnShape` ↔ `ShapeModel` bridge（CP-4.5a）
//!
//! UI 层原有 `DrawnShapeSource` 负责渲染/命中；019–022 的算法吃 `ShapeModel`。
//! 这里只做最小桥接：几何转换 + 保留原音符，不重新生成。

use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeModel, ShapeStyle};
use lumino_editor_state::shape_tool::shape_outline_path;
use lumino_editor_state::{DrawnShape, DrawnShapeSource, ShapeKind as UiShapeKind, ShapeSpec};

/// 选中图形 → ShapeModel（仅几何；音符由调用方保留）
pub(crate) fn model_from_drawn(drawn: &DrawnShape, k: f32) -> ShapeModel {
    model_from_source(&drawn.source, drawn.track, k)
}

/// 几何来源 → ShapeModel（Bake 保留模型 / 选中图形共用）
pub(crate) fn model_from_source(source: &DrawnShapeSource, track: usize, k: f32) -> ShapeModel {
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
            let points = shape_outline_path(spec, 1.0, 1.0);
            model.kind = ShapeKind::Custom;
            model.geometry = ShapeGeometry::from_points(points);
            // 形状工具产物都是闭合图形；保留模型沿用原始填充语义，
            // 载入后 Outline 模型可用同源轮廓音符接管，032B 编辑时再切 Fill。
            model.geometry.closed = true;
            if *filled {
                model.style.fill = lumino_core::shape::ShapeFill::Fill;
            }
        }
    }
    model
}

/// ShapeModel → 可渲染的 DrawnShapeSource
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

#[cfg(test)]
mod tests;
