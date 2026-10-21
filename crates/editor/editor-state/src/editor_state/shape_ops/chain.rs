//! 算子链的两段式执行

use lumino_core::shape::{OpStage, ShapeOp, ShapeRect};

use crate::ShapeNote;

use super::{chop, claw, cycle, gateflip, glue, pattern, strum, sustain, tumours};

/// 几何阶段的上下文（目前只需要拍 → tick 的换算）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpContext {
    /// 每四分音符的 tick 数（PPQ）；必须为正
    pub ppq: f64,
    /// 形状外接框（Polygon 等框型几何需要）；无框为 `None`
    pub rect: Option<ShapeRect>,
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
            rect: None,
        }
    }

    /// 附带形状外接框（Polygon / 框型几何）
    pub fn with_rect(mut self, rect: Option<ShapeRect>) -> Self {
        self.rect = rect;
        self
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
            // Polygon 由调用方按 box 预生成（可能多环）；链上不二次替换路径。
            ShapeOp::Polygon(_) => {}
            ShapeOp::Pattern(params) => {
                current = pattern::formed_path(&current, params, ctx.ppq);
            }
            // Funnel 由 ui-editor 按斜 box 直接生成音符；链上不二次替换路径。
            ShapeOp::Funnel(_) => {}
            // Glue / Claw / Strum / Chop / GateFlip / Sustain 是 Notes 阶段，已被 stage filter 排除。
            ShapeOp::Glue(_)
            | ShapeOp::Claw(_)
            | ShapeOp::Strum(_)
            | ShapeOp::Chop(_)
            | ShapeOp::GateFlip
            | ShapeOp::Sustain(_)
            | ShapeOp::Mask(_)
            | ShapeOp::Cycle(_)
            | ShapeOp::GateRange(_) => {}
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
            ShapeOp::GateFlip => *notes = gateflip::apply(notes),
            // Mask 是标记：区域由 shape_ops::masks 在所有形状的 notes 上统一计算。
            ShapeOp::Mask(_) => {}
            ShapeOp::Cycle(params) => {
                cycle::apply(notes, params, ctx.ppq);
            }
            // GateRange 是标记：spam 生成时消费，不直接改已生成音符。
            ShapeOp::GateRange(_) => {}
            _ => {}
        }
    }
    // Sustain 恒为最后一步（上游明确；只让音符变长），不依赖它在数组中的位置。
    for op in ops {
        if let ShapeOp::Sustain(params) = op {
            *notes = sustain::apply(notes, params, ctx.ppq).0;
        }
    }
    // 其它 Notes 阶段算子（DRAW-010）在此之后按数组顺序应用。
}
