//! 抽屉拖拽草稿（DRAW-033B1）
//!
//! 从 doc.rs 拆出，保持单文件长度纪律；坐标仍为单位框 0..=1。

use lumino_core::shape::{ShapeAnchor, ShapeRect, Stroke, StrokeKind};

use super::{DrawerTool, distance};

/// 多点草稿的阶段（DRAW-033B1a）
///
/// `Second` = 已落第一个点，等待第二个点；`Third` = 已落第二个点，等待第三个点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftStage {
    /// 等待第二个点
    Second,
    /// 等待第三个点
    Third,
}

/// 拖拽草稿
#[derive(Debug, Clone)]
pub enum Draft {
    /// 自由笔
    Freehand {
        /// 采样点
        points: Vec<(f32, f32)>,
    },
    /// 直线（两点拖拽）
    TwoPoint {
        /// 起点
        start: (f32, f32),
        /// 当前点
        current: (f32, f32),
    },
    /// 曲线：起点 → 终点 → 控制点（三段点击）
    Curve {
        /// 起点
        start: (f32, f32),
        /// 终点
        end: (f32, f32),
        /// 控制点（第三段跟随光标）
        control: (f32, f32),
        /// 阶段
        stage: DraftStage,
    },
    /// 三点弧：起点 → 经过点 → 终点（三段点击）
    Arc {
        /// 起点
        start: (f32, f32),
        /// 经过点
        through: (f32, f32),
        /// 终点
        end: (f32, f32),
        /// 阶段
        stage: DraftStage,
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
            DrawerTool::Line => Some(Self::TwoPoint {
                start,
                current: start,
            }),
            DrawerTool::Curve => Some(Self::Curve {
                start,
                end: start,
                control: start,
                stage: DraftStage::Second,
            }),
            DrawerTool::Arc => Some(Self::Arc {
                start,
                through: start,
                end: start,
                stage: DraftStage::Second,
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
            Self::Curve {
                start,
                end,
                control,
                stage,
            } => match stage {
                DraftStage::Second => {
                    *end = point;
                    *control = point;
                }
                DraftStage::Third => {
                    *control = if shift {
                        snap_angle(*start, point, std::f32::consts::PI / 12.0)
                    } else {
                        point
                    }
                }
            },
            Self::Arc {
                through,
                end,
                stage,
                ..
            } => match stage {
                DraftStage::Second => {
                    *through = point;
                    *end = point;
                }
                DraftStage::Third => *end = point,
            },
            Self::Box { start, current, .. } => {
                *current = if shift {
                    constrain_unit_rect(*start, point)
                } else {
                    point
                };
            }
        }
    }

    /// 多点草稿点击：推进阶段；返回 `Some` 表示可以提交。
    pub fn click(&mut self, point: (f32, f32)) -> Option<Stroke> {
        let commit = match self {
            Self::Curve {
                end,
                control,
                stage,
                ..
            } => match stage {
                DraftStage::Second => {
                    *end = point;
                    *control = point;
                    *stage = DraftStage::Third;
                    false
                }
                DraftStage::Third => {
                    *control = point;
                    true
                }
            },
            Self::Arc {
                through,
                end,
                stage,
                ..
            } => match stage {
                DraftStage::Second => {
                    *through = point;
                    *end = point;
                    *stage = DraftStage::Third;
                    false
                }
                DraftStage::Third => {
                    *end = point;
                    true
                }
            },
            _ => return None,
        };
        if commit { self.to_stroke() } else { None }
    }

    /// 是否是多点草稿（Curve/Arc）：释放鼠标不提交，继续等下一击。
    pub fn is_multi_stage(&self) -> bool {
        matches!(self, Self::Curve { .. } | Self::Arc { .. })
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
            Self::TwoPoint { start, current } => {
                if distance(*start, *current) < 1e-3 {
                    return None;
                }
                Some(Stroke {
                    kind: StrokeKind::Line,
                    anchors: vec![ShapeAnchor::new(*start), ShapeAnchor::new(*current)],
                    rect: None,
                })
            }
            Self::Curve {
                start,
                end,
                control,
                stage,
            } => {
                if distance(*start, *end) < 1e-3 {
                    return None;
                }
                match stage {
                    // 第二段：还没落终点，先用直线做引导预览。
                    DraftStage::Second => Some(Stroke {
                        kind: StrokeKind::Line,
                        anchors: vec![ShapeAnchor::new(*start), ShapeAnchor::new(*end)],
                        rect: None,
                    }),
                    // 第三段：二次贝塞尔转三次；控制点由用户第三击确定。
                    DraftStage::Third => {
                        let control_out = (
                            (control.0 - start.0) * 2.0 / 3.0,
                            (control.1 - start.1) * 2.0 / 3.0,
                        );
                        let control_in = (
                            (control.0 - end.0) * 2.0 / 3.0,
                            (control.1 - end.1) * 2.0 / 3.0,
                        );
                        let mut a = ShapeAnchor::new(*start);
                        a.out_handle = control_out;
                        a.handles_auto = false;
                        let mut b = ShapeAnchor::new(*end);
                        b.in_handle = control_in;
                        b.handles_auto = false;
                        Some(Stroke {
                            kind: StrokeKind::Curve,
                            anchors: vec![a, b],
                            rect: None,
                        })
                    }
                }
            }
            Self::Arc {
                start,
                through,
                end,
                stage,
            } => {
                if distance(*start, *end) < 1e-3 {
                    return None;
                }
                match stage {
                    DraftStage::Second => Some(Stroke {
                        kind: StrokeKind::Line,
                        anchors: vec![ShapeAnchor::new(*start), ShapeAnchor::new(*end)],
                        rect: None,
                    }),
                    DraftStage::Third => Some(Stroke {
                        kind: StrokeKind::Arc,
                        anchors: vec![
                            ShapeAnchor::new(*start),
                            ShapeAnchor::new(*through),
                            ShapeAnchor::new(*end),
                        ],
                        rect: None,
                    }),
                }
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

/// Shift 约束的单位框拖拽点：取**较短边**作为边长，保留 dx/dy 符号。
///
/// 与主形状工具的 `effective_rect`（屏幕空间 `constraint_scale` 取短边）
/// 同一口径：Shift 拖出的正方形/正圆不会超出拖拽框的任一轴。
pub(crate) fn constrain_unit_rect(start: (f32, f32), current: (f32, f32)) -> (f32, f32) {
    let dx = current.0 - start.0;
    let dy = current.1 - start.1;
    let side = match (dx.abs() > 0.0, dy.abs() > 0.0) {
        (true, true) => dx.abs().min(dy.abs()),
        (true, false) => dx.abs(),
        (false, true) => dy.abs(),
        (false, false) => 0.0,
    };
    (start.0 + side.copysign(dx), start.1 + side.copysign(dy))
}

/// 把 `center → point` 的方向吸附到 `step` 弧度网格，长度不变
fn snap_angle(center: (f32, f32), point: (f32, f32), step: f32) -> (f32, f32) {
    let dx = point.0 - center.0;
    let dy = point.1 - center.1;
    let length = (dx * dx + dy * dy).sqrt();
    if length < 1e-6 || !step.is_finite() || step <= 0.0 {
        return point;
    }
    let angle = (dy.atan2(dx) / step).round() * step;
    (
        center.0 + angle.cos() * length,
        center.1 + angle.sin() * length,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constrain_unit_rect_uses_short_axis() {
        assert_eq!(
            constrain_unit_rect((0.2, 0.2), (0.7, 0.4)),
            (0.4, 0.4),
            "取较短边：dy=0.2 生效，正方形不超出拖拽框"
        );
        assert_eq!(
            constrain_unit_rect((0.2, 0.2), (0.3, 0.8)),
            (0.3, 0.3),
            "取较短边：dx=0.1 生效"
        );
    }

    #[test]
    fn test_constrain_unit_rect_keeps_direction() {
        assert_eq!(
            constrain_unit_rect((0.8, 0.8), (0.3, 0.6)),
            (0.6, 0.6),
            "反向拖拽保留负方向"
        );
        assert_eq!(constrain_unit_rect((0.8, 0.8), (0.6, 0.3)), (0.6, 0.6));
    }

    #[test]
    fn test_constrain_unit_rect_matches_main_tool_effective_rect() {
        use lumino_editor_state::shape_tool::{ShapeSpec, effective_rect};
        let spec = ShapeSpec {
            kind: lumino_editor_state::ShapeKind::Rectangle,
            rect: (0.2, 0.2, 0.7, 0.4),
            shift_constrained: true,
            apex_high: true,
        };
        let effective = effective_rect(spec, 1.0, 1.0);
        let constrained = constrain_unit_rect((0.2, 0.2), (0.7, 0.4));
        assert_eq!(
            effective,
            (0.2, 0.2, constrained.0, constrained.1),
            "同参数同结果：抽屉与主工具都取较短边"
        );
    }

    #[test]
    fn test_draft_shift_toggles_square_then_release() {
        let mut draft = Draft::begin(DrawerTool::Rect, (0.2, 0.2)).expect("应开始草稿");
        draft.update_with_shift((0.7, 0.4), true);
        let stroke = draft.to_stroke().expect("应有草稿");
        let rect = stroke.rect.expect("矩形应有 rect");
        assert_eq!(rect.size().0, rect.size().1, "Shift 下应是正方形");

        let mut draft = Draft::begin(DrawerTool::Rect, (0.2, 0.2)).expect("应开始草稿");
        draft.update_with_shift((0.7, 0.4), false);
        let stroke = draft.to_stroke().expect("应有草稿");
        let rect = stroke.rect.expect("矩形应有 rect");
        assert_ne!(rect.size().0, rect.size().1, "松开 Shift 应恢复自由比例");
    }

    #[test]
    fn test_curve_three_click_produces_bezier() {
        let mut draft = Draft::begin(DrawerTool::Curve, (0.2, 0.2)).expect("应开始草稿");
        assert!(draft.click((0.8, 0.2)).is_none(), "第二击只落终点");
        draft.update((0.5, 0.8));
        let stroke = draft.click((0.5, 0.8)).expect("第三击应提交");
        assert_eq!(stroke.kind, StrokeKind::Curve);
        assert_eq!(stroke.anchors.len(), 2);
        assert!(
            stroke.anchors[0].out_handle.1.abs() > 0.1,
            "控制点应产生可见的纵向控制柄"
        );
        assert!(!stroke.anchors[0].handles_auto);
    }

    #[test]
    fn test_arc_three_click_keeps_through_point() {
        let mut draft = Draft::begin(DrawerTool::Arc, (0.2, 0.2)).expect("应开始草稿");
        assert!(draft.click((0.5, 0.8)).is_none(), "第二击落经过点");
        draft.update((0.8, 0.2));
        let stroke = draft.click((0.8, 0.2)).expect("第三击应提交");
        assert_eq!(stroke.kind, StrokeKind::Arc);
        assert_eq!(stroke.anchors.len(), 3);
        assert_eq!(stroke.anchors[1].pos, (0.5, 0.8));
    }
}
