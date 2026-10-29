//! 抽屉拖拽草稿（DRAW-033B1）
//!
//! 从 doc.rs 拆出，保持单文件长度纪律；坐标仍为单位框 0..=1。

use lumino_core::shape::{ShapeAnchor, ShapeRect, Stroke, StrokeKind};

use super::{DrawerTool, distance};

/// 拖拽草稿
#[derive(Debug, Clone)]
pub enum Draft {
    /// 自由笔
    Freehand {
        /// 采样点
        points: Vec<(f32, f32)>,
    },
    /// 两点工具（线/曲线）
    TwoPoint {
        /// 工具
        tool: DrawerTool,
        /// 起点
        start: (f32, f32),
        /// 当前点
        current: (f32, f32),
    },
    /// 三点弧
    Arc {
        /// 起点
        start: (f32, f32),
        /// 经过点
        through: (f32, f32),
        /// 终点
        end: (f32, f32),
    },
    /// 框型工具（矩形/圆）
    Box {
        /// 工具
        tool: DrawerTool,
        /// 起点
        start: (f32, f32),
        /// 当前点
        current: (f32, f32),
    },
}

impl Draft {
    /// 开始一个草稿
    pub fn begin(tool: DrawerTool, start: (f32, f32)) -> Option<Self> {
        match tool {
            DrawerTool::Select => None,
            DrawerTool::Pen => Some(Self::Freehand {
                points: vec![start],
            }),
            DrawerTool::Line | DrawerTool::Curve => Some(Self::TwoPoint {
                tool,
                start,
                current: start,
            }),
            DrawerTool::Arc => Some(Self::Arc {
                start,
                through: start,
                end: start,
            }),
            DrawerTool::Rect | DrawerTool::Circle => Some(Self::Box {
                tool,
                start,
                current: start,
            }),
        }
    }

    /// 更新当前点（不按 Shift）
    pub fn update(&mut self, point: (f32, f32)) {
        self.update_with_shift(point, false);
    }

    /// 更新当前点；`shift` 时 Rect/Circle 约束为正方形/正圆
    pub fn update_with_shift(&mut self, point: (f32, f32), shift: bool) {
        match self {
            Self::Freehand { points } => {
                if points.last().is_none_or(|last| {
                    (last.0 - point.0).abs() > 1e-4 || (last.1 - point.1).abs() > 1e-4
                }) {
                    points.push(point);
                }
            }
            Self::TwoPoint { current, .. } => *current = point,
            Self::Arc { through, end, .. } => {
                // 拖拽阶段：当前点既是经过点，也是终点；release 时再一分为二。
                *through = point;
                *end = point;
            }
            Self::Box { start, current, .. } => {
                if shift {
                    let dx = point.0 - start.0;
                    let dy = point.1 - start.1;
                    let side = dx.abs().max(dy.abs());
                    *current = (start.0 + side.copysign(dx), start.1 + side.copysign(dy));
                } else {
                    *current = point;
                }
            }
        }
    }

    /// 草稿预览/提交用的 stroke；拖拽距离过小返回 None
    pub fn to_stroke(&self) -> Option<Stroke> {
        match self {
            Self::Freehand { points } => {
                if points.len() < 2 {
                    return None;
                }
                Some(Stroke {
                    kind: StrokeKind::Freehand,
                    anchors: points.iter().map(|&p| ShapeAnchor::new(p)).collect(),
                    rect: None,
                })
            }
            Self::TwoPoint {
                tool,
                start,
                current,
            } => {
                if distance(*start, *current) < 1e-3 {
                    return None;
                }
                Some(Stroke {
                    kind: if *tool == DrawerTool::Curve {
                        StrokeKind::Curve
                    } else {
                        StrokeKind::Line
                    },
                    anchors: vec![ShapeAnchor::new(*start), ShapeAnchor::new(*current)],
                    rect: None,
                })
            }
            Self::Arc {
                start,
                through,
                end,
            } => {
                if distance(*start, *end) < 1e-3 {
                    return None;
                }
                // 拖拽只给两个点；用中点抬升形成可辨识的三点弧。
                let mid = (
                    (start.0 + end.0) * 0.5 + (through.1 - start.1) * 0.5,
                    (start.1 + end.1) * 0.5 - (through.0 - start.0) * 0.5,
                );
                Some(Stroke {
                    kind: StrokeKind::Arc,
                    anchors: vec![
                        ShapeAnchor::new(*start),
                        ShapeAnchor::new(mid),
                        ShapeAnchor::new(*end),
                    ],
                    rect: None,
                })
            }
            Self::Box {
                tool,
                start,
                current,
            } => {
                let rect = rect_from_drag(*start, *current)?;
                if rect.size().0 < 1e-3 || rect.size().1 < 1e-3 {
                    return None;
                }
                if *tool == DrawerTool::Rect {
                    let corners = [
                        (rect.tick_lo, rect.key_lo),
                        (rect.tick_hi, rect.key_lo),
                        (rect.tick_hi, rect.key_hi),
                        (rect.tick_lo, rect.key_hi),
                        (rect.tick_lo, rect.key_lo),
                    ];
                    Some(Stroke {
                        kind: StrokeKind::Polyline,
                        anchors: corners.into_iter().map(ShapeAnchor::new).collect(),
                        rect: Some(rect),
                    })
                } else {
                    let mut points = Vec::with_capacity(65);
                    for index in 0..=64 {
                        let angle = std::f32::consts::TAU * index as f32 / 64.0;
                        points.push((
                            (rect.tick_lo + rect.tick_hi) * 0.5
                                + (rect.tick_hi - rect.tick_lo) * 0.5 * angle.cos(),
                            (rect.key_lo + rect.key_hi) * 0.5
                                + (rect.key_hi - rect.key_lo) * 0.5 * angle.sin(),
                        ));
                    }
                    Some(Stroke {
                        kind: StrokeKind::Polyline,
                        anchors: points.into_iter().map(ShapeAnchor::new).collect(),
                        rect: Some(rect),
                    })
                }
            }
        }
    }
}

/// 由两个单位框点构造规范矩形
fn rect_from_drag(a: (f32, f32), b: (f32, f32)) -> Option<ShapeRect> {
    if !a.0.is_finite() || !a.1.is_finite() || !b.0.is_finite() || !b.1.is_finite() {
        return None;
    }
    Some(ShapeRect::normalized(a.0, a.1, b.0, b.1))
}
