//! Custom edit 形状变换（DRAW-020）
//!
//! 纯手柄/参数变换：命中测试与拖动都只改 `ShapeModel.geometry` 的锚点/框，
//! 不烘焙绝对点列。UI 拖拽预览/撤销接线留给交互卡。

use std::f32::consts::PI;

use lumino_core::shape::{ShapeKind, ShapeModel, ShapeOp, ShapeRect};

mod group;

use group::box_op;
pub use group::{apply_group_drag, union_bounds, union_model};

/// 缩放下限：角手柄拖过对边时比例会变负，直接镜像图形；编辑态钳到极小正值，
/// 表现成「缩到底停住」，不再无提示翻转。
const MIN_SCALE: f32 = 0.01;

/// 逻辑坐标（tick, key）
pub type Pt = (f32, f32);

/// 屏幕缩放（每 tick / 每 key 像素）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zoom {
    /// 每 tick 像素
    pub x: f32,
    /// 每 key 像素
    pub y: f32,
}

/// 修饰键
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    /// Ctrl：角点等比缩放
    pub ctrl: bool,
    /// Shift：关闭旋转 15° 吸附
    pub shift: bool,
}

/// 角点
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    /// 左上
    Nw,
    /// 右上
    Ne,
    /// 左下
    Sw,
    /// 右下
    Se,
}

/// 单边
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// 左边
    Left,
    /// 右边
    Right,
    /// 上边
    Top,
    /// 下边
    Bottom,
}

/// 手柄类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    /// 整体移动
    Move,
    /// 角点缩放
    ResizeCorner(Corner),
    /// 单边拉伸
    Side(Side),
    /// 旋转
    Turn,
    /// 倾斜
    Skew,
    /// 贝塞尔锚点/手柄
    AnchorHandle(u16),
    /// 路径点
    Path,
}

/// 手柄布局
#[derive(Debug, Clone, PartialEq)]
pub struct EditHandles {
    /// 归一化 box（u0,v0,u1,v1）
    pub box_uv: [f32; 4],
    /// 手柄与逻辑坐标
    pub handles: Vec<(HandleKind, Pt)>,
}

/// 计算手柄布局
/// 箱体手柄：4 角 + 4 边，`include_center` 时额外加中心 Move。
///
/// 020 编辑态严格使用 8+1；曲线 Path 点不在此列。
pub fn box_handles(shape: &ShapeModel, _zoom: Zoom, include_center: bool) -> Option<EditHandles> {
    let (x0, y0, x1, y1) = bounds(shape)?;
    let cx = (x0 + x1) / 2.0;
    let cy = (y0 + y1) / 2.0;
    let mut handles = vec![
        (HandleKind::ResizeCorner(Corner::Nw), (x0, y1)),
        (HandleKind::ResizeCorner(Corner::Ne), (x1, y1)),
        (HandleKind::ResizeCorner(Corner::Sw), (x0, y0)),
        (HandleKind::ResizeCorner(Corner::Se), (x1, y0)),
        (HandleKind::Side(Side::Left), (x0, cy)),
        (HandleKind::Side(Side::Right), (x1, cy)),
        (HandleKind::Side(Side::Bottom), (cx, y0)),
        (HandleKind::Side(Side::Top), (cx, y1)),
    ];
    if include_center {
        handles.push((HandleKind::Move, (cx, cy)));
    }
    Some(EditHandles {
        box_uv: [0.0, 0.0, 1.0, 1.0],
        handles,
    })
}

/// 全部手柄（箱体 + 曲线路径点），供纯函数/曲线工具使用。
pub fn handles(shape: &ShapeModel, zoom: Zoom) -> Option<EditHandles> {
    let mut layout = box_handles(shape, zoom, false)?;
    if matches!(shape.kind, ShapeKind::Curve) {
        for (index, anchor) in shape.geometry.anchors.iter().enumerate() {
            layout
                .handles
                .push((HandleKind::AnchorHandle(index as u16), anchor.pos));
        }
    } else if matches!(shape.kind, ShapeKind::Polyline | ShapeKind::Freehand) {
        for anchor in &shape.geometry.anchors {
            layout.handles.push((HandleKind::Path, anchor.pos));
        }
    }
    Some(layout)
}

/// 命中箱体手柄（不含 Path 点）。
pub fn hit_test_box(
    shape: &ShapeModel,
    p: Pt,
    zoom: Zoom,
    tol_px: f32,
    include_center: bool,
) -> Option<HandleKind> {
    let layout = box_handles(shape, zoom, include_center)?;
    hit_test_layout(&layout, p, zoom, tol_px)
}

/// 命中最近手柄（像素容差）
pub fn hit_test(shape: &ShapeModel, p: Pt, zoom: Zoom, tol_px: f32) -> Option<HandleKind> {
    let layout = handles(shape, zoom)?;
    hit_test_layout(&layout, p, zoom, tol_px)
}

fn hit_test_layout(layout: &EditHandles, p: Pt, zoom: Zoom, tol_px: f32) -> Option<HandleKind> {
    layout
        .handles
        .iter()
        .filter_map(|(kind, pos)| {
            let dx = (p.0 - pos.0) * zoom.x;
            let dy = (p.1 - pos.1) * zoom.y;
            let d = (dx * dx + dy * dy).sqrt();
            let priority = match kind {
                HandleKind::AnchorHandle(_) | HandleKind::Path => 0,
                _ => 1,
            };
            (d <= tol_px).then_some((d, priority, *kind))
        })
        .min_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        })
        .map(|(_, _, kind)| kind)
}

/// 应用拖动；成功返回 `true`
pub fn apply_drag(
    shape: &mut ShapeModel,
    kind: HandleKind,
    from: Pt,
    to: Pt,
    mods: Mods,
    k: f32,
) -> bool {
    let Some(bounds) = bounds(shape) else {
        return false;
    };
    if let Some(op) = box_op(bounds, kind, from, to, mods, k) {
        return transform(shape, |x, y| op.apply(x, y));
    }
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    match kind {
        HandleKind::AnchorHandle(index) => {
            let Some(anchor) = shape.geometry.anchors.get_mut(usize::from(index)) else {
                return false;
            };
            anchor.pos = (anchor.pos.0 + dx, anchor.pos.1 + dy);
            true
        }
        HandleKind::Path => {
            let Some(anchor) = shape.geometry.anchors.iter_mut().min_by(|a, b| {
                let da = dist2(a.pos, from);
                let db = dist2(b.pos, from);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            }) else {
                return false;
            };
            anchor.pos = (anchor.pos.0 + dx, anchor.pos.1 + dy);
            true
        }
        _ => false,
    }
}

fn transform(shape: &mut ShapeModel, f: impl Fn(f32, f32) -> Pt) -> bool {
    transform_model(shape, &f);
    true
}

fn transform_model(shape: &mut ShapeModel, f: &impl Fn(f32, f32) -> Pt) {
    let old_origin = shape.geometry.bounds().map(|bounds| (bounds.0, bounds.1));
    for anchor in &mut shape.geometry.anchors {
        anchor.pos = f(anchor.pos.0, anchor.pos.1);
    }
    if let Some(rect) = &mut shape.geometry.rect {
        let corners = [
            (rect.tick_lo, rect.key_lo),
            (rect.tick_hi, rect.key_lo),
            (rect.tick_hi, rect.key_hi),
            (rect.tick_lo, rect.key_hi),
        ]
        .map(|(x, y)| f(x, y));
        let mut nx0 = f32::INFINITY;
        let mut ny0 = f32::INFINITY;
        let mut nx1 = f32::NEG_INFINITY;
        let mut ny1 = f32::NEG_INFINITY;
        for (x, y) in corners {
            nx0 = nx0.min(x);
            ny0 = ny0.min(y);
            nx1 = nx1.max(x);
            ny1 = ny1.max(y);
        }
        *rect = ShapeRect {
            tick_lo: nx0,
            key_lo: ny0,
            tick_hi: nx1,
            key_hi: ny1,
        };
    }
    // 带绝对坐标的 fx 页必须跟着变换，否则重生成时副本/内嵌笔画会留在旧位置。
    for op in &mut shape.ops {
        match op {
            ShapeOp::CopyAlong(params) => {
                // grp / pts 是「绝对起点 + 相对向量」；只变换 grp 会让旋转/缩放后的
                // 副本路径仍沿旧方向，表现为副本扭曲/飞走。
                let origin = params
                    .grp
                    .map(|grp| (grp[0], grp[1]))
                    .or(old_origin)
                    .unwrap_or((0.0, 0.0));
                let new_origin = f(origin.0, origin.1);
                if let Some(grp) = &mut params.grp {
                    *grp = [new_origin.0, new_origin.1];
                }
                for pt in &mut params.pts {
                    let (x, y) = f(origin.0 + pt[0], origin.1 + pt[1]);
                    pt[0] = x - new_origin.0;
                    pt[1] = y - new_origin.1;
                }
                for mid in params.mids.iter_mut().flatten() {
                    let (x, y) = f(origin.0 + mid[0], origin.1 + mid[1]);
                    mid[0] = x - new_origin.0;
                    mid[1] = y - new_origin.1;
                }
            }
            ShapeOp::Live(params) => {
                for stroke in &mut params.strokes {
                    for anchor in &mut stroke.anchors {
                        anchor.pos = f(anchor.pos.0, anchor.pos.1);
                    }
                }
                for from in &mut params.from {
                    transform_model(from, f);
                }
            }
            _ => {}
        }
    }
}

fn ratio(to: f32, fixed: f32, from: f32) -> f32 {
    let denom = from - fixed;
    if denom.abs() < 1e-6 {
        1.0
    } else {
        (to - fixed) / denom
    }
}

fn zoom_scale(k: f32) -> f32 {
    if k.is_finite() && k > 0.0 { k } else { 1.0 }
}

fn bounds(shape: &ShapeModel) -> Option<(f32, f32, f32, f32)> {
    shape.geometry.bounds()
}

fn dist2(a: Pt, b: Pt) -> f32 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

#[cfg(test)]
mod tests;
