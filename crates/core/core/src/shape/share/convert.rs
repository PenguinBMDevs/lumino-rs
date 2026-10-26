//! 分享载荷的 tick ↔ beats 转换
//!
//! `ShapeModel` 的几何坐标是 tick；分享文本要求 beats。这里把**所有** tick 字段
//! 逐个换算，key/百分比/拍参数保持原值。新增带 tick 字段的算子时必须同步扩展本文件，
//! 否则跨 PPQ 分享会在无声中改变音乐位置。

use super::ShareError;
use crate::shape::{ShapeAnchor, ShapeGeometry, ShapeModel, ShapeOp, ShapeRect, SymAxis};

/// 最大嵌套层数（`Live.from` 递归）；正常形状远低于此值。
const MAX_DEPTH: usize = 16;

pub(super) fn to_beats(shape: &mut ShapeModel, ppq: u16) -> Result<(), ShareError> {
    scale_shape(shape, 1.0 / f64::from(ppq), 0)
}

pub(super) fn from_beats(shape: &mut ShapeModel, ppq: u16) -> Result<(), ShareError> {
    scale_shape(shape, f64::from(ppq), 0)
}

fn scale_shape(shape: &mut ShapeModel, factor: f64, depth: usize) -> Result<(), ShareError> {
    if depth > MAX_DEPTH {
        return Err(ShareError::Convert("Live 嵌套过深".into()));
    }
    scale_geometry(&mut shape.geometry, factor);
    for op in &mut shape.ops {
        scale_op(op, factor, depth)?;
    }
    Ok(())
}

fn scale_geometry(geometry: &mut ShapeGeometry, factor: f64) {
    for anchor in &mut geometry.anchors {
        scale_anchor(anchor, factor);
    }
    if let Some(rect) = &mut geometry.rect {
        scale_rect(rect, factor);
    }
}

fn scale_anchor(anchor: &mut ShapeAnchor, factor: f64) {
    scale_tick(&mut anchor.pos.0, factor);
    scale_tick(&mut anchor.in_handle.0, factor);
    scale_tick(&mut anchor.out_handle.0, factor);
}

fn scale_rect(rect: &mut ShapeRect, factor: f64) {
    scale_tick(&mut rect.tick_lo, factor);
    scale_tick(&mut rect.tick_hi, factor);
}

fn scale_op(op: &mut ShapeOp, factor: f64, depth: usize) -> Result<(), ShareError> {
    match op {
        ShapeOp::Tiles(params) => {
            scale_tick(&mut params.length, factor);
            scale_tick(&mut params.gap, factor);
        }
        ShapeOp::Symmetry(params) => {
            if let SymAxis::Vertical { x } = &mut params.axis {
                scale_tick(x, factor);
            }
        }
        ShapeOp::Text(op) => scale_tick(&mut op.snap, factor),
        ShapeOp::Funnel(params) => {
            for line in &mut params.lines {
                for point in line {
                    scale_point(point, factor);
                }
            }
            for point in &mut params.wall {
                scale_point(point, factor);
            }
            for start in &mut params.starts {
                for curve in start.ends.iter_mut().flatten() {
                    for point in curve {
                        scale_point(point, factor);
                    }
                }
            }
        }
        ShapeOp::CopyAlong(params) => {
            for point in &mut params.pts {
                scale_point(point, factor);
            }
            for point in params.mids.iter_mut().flatten() {
                scale_point(point, factor);
            }
            if let Some(point) = &mut params.grp {
                scale_point(point, factor);
            }
        }
        ShapeOp::Live(params) => {
            for stroke in &mut params.strokes {
                for anchor in &mut stroke.anchors {
                    scale_anchor(anchor, factor);
                }
                if let Some(rect) = &mut stroke.rect {
                    scale_rect(rect, factor);
                }
            }
            for source in &mut params.from {
                scale_shape(source, factor, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn scale_point(point: &mut [f32; 2], factor: f64) {
    scale_tick(&mut point[0], factor);
}

fn scale_tick(value: &mut f32, factor: f64) {
    if value.is_finite() {
        *value = (*value as f64 * factor) as f32;
    }
}
