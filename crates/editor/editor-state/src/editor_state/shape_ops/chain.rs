//! 算子链的两段式执行

use lumino_core::shape::{OpStage, ShapeOp, ShapeRect};

use crate::ShapeNote;

use super::{chop, claw, glue, strum, tumours};

/// 几何阶段的上下文（目前只需要拍 → tick 的换算）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpContext {
    /// 每四分音符的 tick 数（PPQ）；必须为正
    pub ppq: f64,
}

impl OpContext {
    /// 构造上下文（非法 PPQ 会被夹到极小正值，避免除零）
    pub fn new(ppq: f64) -> Self {
        Self {
            ppq: if ppq.is_finite() && ppq > 0.0 {
                ppq
            } else {
                f64::EPSILON
            },
        }
    }
}

/// 依次应用 Geometry 阶段算子；`path` 为展平后的 `(tick, key)` 点列
pub fn apply_geometry_ops(
    path: &[(f64, f64)],
    ops: &[ShapeOp],
    ctx: &OpContext,
) -> Vec<(f64, f64)> {
    let mut current = path.to_vec();
    for op in ops.iter().filter(|op| op.stage() == OpStage::Geometry) {
        match op {
            ShapeOp::Noop => {}
            ShapeOp::Tumours(params) => {
                current = tumours::apply(&current, params, ctx.ppq).0;
            }
            // Glue / Claw / Strum / Chop 是 Notes 阶段，已被上面的 stage filter 排除。
            ShapeOp::Glue(_) | ShapeOp::Claw(_) | ShapeOp::Strum(_) | ShapeOp::Chop(_) => {}
        }
    }
    current
}

/// Notes 阶段的上下文
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NoteOpContext {
    /// 形状包围盒（`Boxes` 的相对坐标系）；无几何时为 `None`
    pub glue_box: Option<ShapeRect>,
    /// 拍 → tick 的换算（Claw time 模式需要）；非法/0 时相关算子原样返回
    pub ppq: f64,
}

/// 依次应用 Notes 阶段算子
///
/// **Glue 恒为第一位**（上游顺序 `glue → claw/strum/chop → …`），不依赖它在
/// `ops` 数组中的位置；其余 Notes 算子（DRAW-004..010）在 Glue 之后按数组顺序应用。
pub fn apply_note_ops(notes: &mut Vec<ShapeNote>, ops: &[ShapeOp], ctx: &NoteOpContext) {
    for op in ops {
        if let ShapeOp::Glue(params) = op {
            *notes = glue::apply(notes, params, ctx.glue_box);
        }
    }
    for op in ops {
        match op {
            ShapeOp::Claw(params) => *notes = claw::apply(notes, params, ctx.ppq),
            ShapeOp::Strum(params) => *notes = strum::apply(notes, params, ctx.ppq),
            ShapeOp::Chop(params) => *notes = chop::apply(notes, params, ctx.ppq).0,
            _ => {}
        }
    }
    // 其它 Notes 阶段算子（DRAW-009/008/010）在此之后按数组顺序应用。
}
