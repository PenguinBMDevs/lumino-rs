//! DRAW-020：箱体仿射算子与多选组变换
//!
//! 单形状与多选组共用同一套箱体语义；`apply_group_drag` 以成员联合 box
//! 为基准，把每个成员参数按同一算子映射，不烘焙点列。

use lumino_core::shape::ShapeGeometry;

use super::*;

/// 箱体手柄的仿射算子（单形状与组变换共用同一套语义）
#[derive(Debug, Clone, Copy)]
pub(super) enum BoxOp {
    /// 整体平移
    Translate { dx: f32, dy: f32 },
    /// 角点/单边缩放（固定边或对角点）
    Scale { fx: f32, fy: f32, sx: f32, sy: f32 },
    /// 屏幕空间旋转（含 k 比例）
    Rotate {
        cx: f32,
        cy: f32,
        sin: f32,
        cos: f32,
        scale: f32,
    },
    /// 整格倾斜
    Skew { shear: f32, center: f32 },
}

impl BoxOp {
    pub(super) fn apply(self, x: f32, y: f32) -> Pt {
        match self {
            Self::Translate { dx, dy } => (x + dx, y + dy),
            Self::Scale { fx, fy, sx, sy } => (fx + (x - fx) * sx, fy + (y - fy) * sy),
            Self::Rotate {
                cx,
                cy,
                sin,
                cos,
                scale,
            } => {
                let dx = x - cx;
                let dy = y - cy;
                (
                    cx + dx * cos - dy * scale * sin,
                    cy + dx * sin / scale + dy * cos,
                )
            }
            Self::Skew { shear, center } => (x + shear * (y - center), y),
        }
    }
}

/// 由 box 手柄 + 拖动起止点构造仿射算子；非 box 手柄返回 `None`
pub(super) fn box_op(
    bounds: (f32, f32, f32, f32),
    kind: HandleKind,
    from: Pt,
    to: Pt,
    mods: Mods,
    k: f32,
) -> Option<BoxOp> {
    let (x0, y0, x1, y1) = bounds;
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    Some(match kind {
        HandleKind::Move => BoxOp::Translate { dx, dy },
        HandleKind::ResizeCorner(corner) => {
            let (fx, fy) = match corner {
                Corner::Nw => (x1, y0),
                Corner::Ne => (x0, y0),
                Corner::Sw => (x1, y1),
                Corner::Se => (x0, y1),
            };
            // 拖过对边时 ratio 会变负 —— 那会把图形直接镜像（上下/左右翻转）。
            // 编辑态要求「拖到底就停住」，不允许无提示翻转，因此钳到最小正比例；
            // 需要镜像请用 023 半边对称。
            let mut sx = ratio(to.0, fx, from.0).max(MIN_SCALE);
            let mut sy = ratio(to.1, fy, from.1).max(MIN_SCALE);
            if mods.ctrl {
                let s = ((sx + sy) / 2.0).max(MIN_SCALE);
                sx = s;
                sy = s;
            }
            BoxOp::Scale { fx, fy, sx, sy }
        }
        HandleKind::Side(side) => {
            let width = (x1 - x0).max(1e-6);
            let height = (y1 - y0).max(1e-6);
            let (sx, sy) = match side {
                Side::Left => (((x1 - to.0) / width).max(0.01), 1.0),
                Side::Right => (((to.0 - x0) / width).max(0.01), 1.0),
                Side::Bottom => (1.0, ((y1 - to.1) / height).max(0.01)),
                Side::Top => (1.0, ((to.1 - y0) / height).max(0.01)),
            };
            let fx = if matches!(side, Side::Left) { x1 } else { x0 };
            let fy = if matches!(side, Side::Bottom) { y1 } else { y0 };
            BoxOp::Scale { fx, fy, sx, sy }
        }
        HandleKind::Turn => {
            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let a0 = ((from.1 - cy) * zoom_scale(k)).atan2(from.0 - cx);
            let a1 = ((to.1 - cy) * zoom_scale(k)).atan2(to.0 - cx);
            let mut delta = a1 - a0;
            if !mods.shift {
                let snap = PI / 12.0;
                delta = (delta / snap).round() * snap;
            }
            let (sin, cos) = delta.sin_cos();
            BoxOp::Rotate {
                cx,
                cy,
                sin,
                cos,
                scale: zoom_scale(k),
            }
        }
        HandleKind::Skew => {
            let shear = ((to.0 - from.0) / k.max(0.01)).round() * k.max(0.01);
            BoxOp::Skew {
                shear,
                center: (y0 + y1) / 2.0,
            }
        }
        HandleKind::AnchorHandle(_) | HandleKind::Path => return None,
    })
}

/// 多个形状的联合包围盒 (x0, y0, x1, y1)；空集返回 `None`
pub fn union_bounds(models: &[ShapeModel]) -> Option<(f32, f32, f32, f32)> {
    let mut iter = models.iter().filter_map(bounds);
    let first = iter.next()?;
    Some(iter.fold(first, |a, b| {
        (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
    }))
}

/// 组变换的虚拟载体：联合 box 的矩形模型（供手柄布局与命中测试）
pub fn union_model(models: &[ShapeModel]) -> Option<ShapeModel> {
    let (x0, y0, x1, y1) = union_bounds(models)?;
    let first = models.first()?;
    let mut model = ShapeModel::new(0, first.track, ShapeKind::Custom, first.k);
    model.geometry = ShapeGeometry::from_rect(ShapeRect {
        tick_lo: x0,
        key_lo: y0,
        tick_hi: x1,
        key_hi: y1,
    });
    Some(model)
}

/// 组变换：对每个成员应用同一个箱体算子（参数写回，不烘焙点列）
///
/// 组的 box = 成员联合包围盒；每个成员按同一 `BoxOp` 映射，
/// 因此相对布局（成员在组内的位置/尺寸比例）保持不变。
pub fn apply_group_drag(
    models: &mut [ShapeModel],
    kind: HandleKind,
    from: Pt,
    to: Pt,
    mods: Mods,
    k: f32,
) -> bool {
    if models.is_empty() {
        return false;
    }
    let Some(bounds) = union_bounds(models) else {
        return false;
    };
    let Some(op) = box_op(bounds, kind, from, to, mods, k) else {
        return false;
    };
    for model in models.iter_mut() {
        transform_model(model, &|x, y| op.apply(x, y));
    }
    true
}
