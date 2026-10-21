//! 力度工具与包络应用（DRAW-028）

use std::collections::BTreeMap;

use lumino_core::shape::expr::compile;
use lumino_core::shape::{VelocityEnv, env_at, tidy_env};

use crate::ShapeNote;

/// Curve 采样数
pub const CURVE_STEPS: usize = 48;
/// 公式初始高度（力度步）
pub const VEL_HEIGHT: f32 = 20.0;
/// 默认圈数
pub const VEL_LOOPS: f32 = 4.0;
/// 圈数上限
pub const VEL_MOST_LOOPS: f32 = 10_000.0;
/// 公式采样点
pub const FORMULA_POINTS: usize = 4000;
/// 每圈最少点数
pub const FEWEST_PER_LOOP: usize = 4;

/// 力度工具
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VelTool {
    /// 两点直线
    Linear,
    /// 带控制点的曲线
    Curve,
    /// 自由绘
    Pencil,
    /// 公式摆动
    Formula,
}

/// 一次拖动（u, velocity）
#[derive(Debug, Clone, PartialEq)]
pub struct Drag {
    /// 起点
    pub from: (f32, f32),
    /// 终点
    pub to: (f32, f32),
    /// Formula 工具使用的公式
    pub formula: Option<String>,
}

impl Drag {
    /// 线性拖动
    pub fn new(from: (f32, f32), to: (f32, f32)) -> Self {
        Self {
            from,
            to,
            formula: None,
        }
    }
}

/// 应用工具，返回更新后的包络点
pub fn apply_vel_tool(
    env: &mut VelocityEnv,
    tool: VelTool,
    drag: &Drag,
    _ppq: u32,
) -> Vec<[f32; 2]> {
    match tool {
        VelTool::Linear => {
            env.points = vec![
                [drag.from.0.clamp(0.0, 1.0), clamp(drag.from.1)],
                [drag.to.0.clamp(0.0, 1.0), clamp(drag.to.1)],
            ];
        }
        VelTool::Curve => {
            let (x0, y0) = (drag.from.0.clamp(0.0, 1.0), clamp(drag.from.1));
            let (x1, y1) = (drag.to.0.clamp(0.0, 1.0), clamp(drag.to.1));
            let mid = ((x0 + x1) / 2.0, (y0 + y1) / 2.0 + VEL_HEIGHT);
            env.points = (0..=CURVE_STEPS)
                .map(|index| {
                    let t = index as f32 / CURVE_STEPS as f32;
                    let a = (1.0 - t) * (1.0 - t);
                    let b = 2.0 * (1.0 - t) * t;
                    let c = t * t;
                    [a * x0 + b * mid.0 + c * x1, a * y0 + b * mid.1 + c * y1]
                })
                .collect();
        }
        VelTool::Pencil => {
            let mut points = Vec::new();
            for index in 0..=16 {
                let t = index as f32 / 16.0;
                points.push([
                    drag.from.0 + (drag.to.0 - drag.from.0) * t,
                    drag.from.1 + (drag.to.1 - drag.from.1) * t,
                ]);
            }
            env.points = points;
        }
        VelTool::Formula => {
            let src = drag
                .formula
                .as_deref()
                .unwrap_or("50 + height * sin(x * 2 * pi * 4)");
            let vars = BTreeMap::from([("height".to_string(), f64::from(VEL_HEIGHT))]);
            let expr = compile(src, &["height"]).ok();
            env.points = (0..FORMULA_POINTS)
                .map(|index| {
                    let u = index as f32 / (FORMULA_POINTS - 1) as f32;
                    let value = expr
                        .as_ref()
                        .and_then(|expr| expr.eval(f64::from(u), &vars).ok())
                        .unwrap_or(100.0 - f64::from(VEL_HEIGHT));
                    [u, value as f32]
                })
                .collect();
        }
    }
    tidy_env(env);
    env.points.clone()
}

/// 把包络写回音符；`u` 以这组音符的最早/最晚 tick 为 0..1
pub fn apply_env_to_notes(notes: &mut [ShapeNote], env: &VelocityEnv) {
    let Some((lo, hi)) = span(notes) else {
        return;
    };
    let width = (hi - lo).max(1e-6);
    for note in notes {
        let u = (note.tick - lo) / width;
        note.velocity = env_at(env, u, false);
    }
}

/// ShapeOp 入口
pub fn apply(notes: &[ShapeNote], env: &VelocityEnv, _ppq: f64) -> Vec<ShapeNote> {
    let mut out = notes.to_vec();
    apply_env_to_notes(&mut out, env);
    out
}

fn span(notes: &[ShapeNote]) -> Option<(f32, f32)> {
    let lo = notes
        .iter()
        .map(|note| note.tick)
        .fold(f32::INFINITY, f32::min);
    let hi = notes
        .iter()
        .map(|note| note.tick + note.length)
        .fold(f32::NEG_INFINITY, f32::max);
    (hi > lo).then_some((lo, hi))
}

fn clamp(value: f32) -> f32 {
    value.round().clamp(1.0, 127.0)
}

#[cfg(test)]
mod tests;
