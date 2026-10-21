//! 半边对称展开（DRAW-023）
//!
//! 输入是**一半**锚点（最后一个锚点在对称轴上），输出 `2n-1` 个锚点：
//! 前半原样保留，后半由前半镜像/转半圈得到。只支持 Curve；Joined 曲线拒绝。

use lumino_core::shape::{ShapeAnchor, ShapeKind, ShapeModel, ShapeOp, Sym, SymAxis, SymParams};

/// 展开半边对称；非法输入返回 `None`（不 panic、不猜）
pub fn symmetric(anchors: &[ShapeAnchor], params: &SymParams) -> Option<Vec<ShapeAnchor>> {
    let n = anchors.len();
    if n < 3 || n.is_multiple_of(2) {
        return None;
    }
    params.validate().ok()?;
    let axis = Axis::new(params.axis)?;
    let pivot = axis.onto_line(anchors[n - 1].pos);
    if !finite(pivot) {
        return None;
    }
    let mut out = anchors.to_vec();
    let last = out.last_mut()?;
    last.pos = pivot;
    axis.flat_handles(last);
    for index in (0..n - 1).rev() {
        let source = anchors[index];
        let mut reflected = source;
        match params.mode {
            Sym::Mirror => {
                reflected.pos = axis.reflect(source.pos);
                // 反向遍历：原来的出柄变入柄，原来的入柄变出柄
                reflected.in_handle = axis.reflect_vec(source.out_handle);
                reflected.out_handle = axis.reflect_vec(source.in_handle);
            }
            Sym::Turn => {
                reflected.pos = (2.0 * pivot.0 - source.pos.0, 2.0 * pivot.1 - source.pos.1);
                reflected.in_handle = (-source.out_handle.0, -source.out_handle.1);
                reflected.out_handle = (-source.in_handle.0, -source.in_handle.1);
            }
        }
        if !finite(reflected.pos) || !finite(reflected.in_handle) || !finite(reflected.out_handle) {
            return None;
        }
        out.push(reflected);
    }
    // 接缝：轴上的锚点出柄与后半第一段的入柄压平到轴方向
    let seam = n - 1;
    axis.flat_handles(&mut out[seam]);
    if let Some(first_reflected) = out.get_mut(n) {
        axis.flat_in_handle(first_reflected);
    }
    Some(out)
}

/// 形状级入口：只允许 Curve，Joined 曲线禁用
pub fn symmetric_shape(model: &ShapeModel, params: &SymParams) -> Option<Vec<ShapeAnchor>> {
    if model.kind != ShapeKind::Curve || model.ops.iter().any(|op| matches!(op, ShapeOp::Joined(_)))
    {
        return None;
    }
    symmetric(&model.geometry.anchors, params)
}

#[derive(Debug, Clone, Copy)]
struct Axis {
    origin: (f32, f32),
    dir: (f32, f32),
    len2: f32,
}

impl Axis {
    fn new(axis: SymAxis) -> Option<Self> {
        let (origin, dir) = match axis {
            SymAxis::Vertical { x } => ((x, 0.0), (0.0, 1.0)),
            SymAxis::Horizontal { y } => ((0.0, y), (1.0, 0.0)),
            SymAxis::Custom { origin, dir } => (origin, dir),
        };
        let len2 = dir.0 * dir.0 + dir.1 * dir.1;
        (len2.is_finite() && len2 > 1e-12).then_some(Self { origin, dir, len2 })
    }

    fn along(&self, v: (f32, f32)) -> f32 {
        (v.0 * self.dir.0 + v.1 * self.dir.1) / self.len2
    }

    fn onto_line(&self, p: (f32, f32)) -> (f32, f32) {
        let t = self.along((p.0 - self.origin.0, p.1 - self.origin.1));
        (
            self.origin.0 + self.dir.0 * t,
            self.origin.1 + self.dir.1 * t,
        )
    }

    fn reflect_vec(&self, v: (f32, f32)) -> (f32, f32) {
        let t = self.along(v);
        (2.0 * self.dir.0 * t - v.0, 2.0 * self.dir.1 * t - v.1)
    }

    fn reflect(&self, p: (f32, f32)) -> (f32, f32) {
        let projected = self.onto_line(p);
        (2.0 * projected.0 - p.0, 2.0 * projected.1 - p.1)
    }

    fn flat_vec(&self, v: (f32, f32)) -> (f32, f32) {
        let t = self.along(v);
        (self.dir.0 * t, self.dir.1 * t)
    }

    fn flat_handles(&self, anchor: &mut ShapeAnchor) {
        anchor.in_handle = self.flat_vec(anchor.in_handle);
        anchor.out_handle = self.flat_vec(anchor.out_handle);
        anchor.handles_auto = false;
    }

    fn flat_in_handle(&self, anchor: &mut ShapeAnchor) {
        anchor.in_handle = self.flat_vec(anchor.in_handle);
        anchor.handles_auto = false;
    }
}

fn finite(p: (f32, f32)) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

#[cfg(test)]
mod tests;
