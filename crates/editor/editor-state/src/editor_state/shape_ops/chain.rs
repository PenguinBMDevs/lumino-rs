//! 算子链的两段式执行

use lumino_core::shape::{OpStage, ShapeOp};

use crate::ShapeNote;

use super::tumours;

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
        }
    }
    current
}

/// 依次应用 Notes 阶段算子
///
/// 当前还没有 Notes 阶段算子（Tumours 属 Geometry）；接口先落位，
/// DRAW-004..010 只需在这里追加分支，不改调用方。
pub fn apply_note_ops(notes: &mut [ShapeNote], ops: &[ShapeOp]) {
    let _ = notes;
    for op in ops.iter().filter(|op| op.stage() == OpStage::Notes) {
        match op {
            ShapeOp::Noop | ShapeOp::Tumours(_) => {
                // Tumours 是 Geometry 阶段，已被上面的 filter 排除。
            }
        }
    }
}
