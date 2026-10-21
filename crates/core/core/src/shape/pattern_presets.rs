//! Pattern 预设（DRAW-016）
//!
//! 先落 12 个与上游 `PATTERN_PRESETS` 对齐的公式；UI 卡只需引用这张表。

use std::collections::BTreeMap;

use super::{PatternParams, Sym};

/// 一个公式预设
#[derive(Debug, Clone, Copy)]
pub struct PatternPreset {
    /// 稳定 id
    pub id: &'static str,
    /// 显示名
    pub name: &'static str,
    /// y = f(x)
    pub formula: &'static str,
    /// 变量默认值
    pub vars: &'static [(&'static str, f64)],
    /// 可选的 along 公式
    pub along: Option<&'static str>,
}

/// 12 个内置预设
pub const PATTERN_PRESETS: &[PatternPreset] = &[
    PatternPreset {
        id: "wave",
        name: "Wave",
        formula: "height * sin(x * 2 * pi)",
        vars: &[("height", 4.0)],
        along: None,
    },
    PatternPreset {
        id: "zigzag",
        name: "Zigzag",
        formula: "height * 2 / pi * asin(sin(x * 2 * pi))",
        vars: &[("height", 4.0)],
        along: None,
    },
    PatternPreset {
        id: "bounce",
        name: "Bounce",
        formula: "height * abs(sin(x * pi))",
        vars: &[("height", 4.0)],
        along: None,
    },
    PatternPreset {
        id: "square",
        name: "Square wave",
        formula: "height * max(-1, min(1, steep * sin(x * 2 * pi)))",
        vars: &[("height", 4.0), ("steep", 8.0)],
        along: None,
    },
    PatternPreset {
        id: "saw",
        name: "Sawtooth",
        formula: "height * 2 * (x - floor(x + 0.5))",
        vars: &[("height", 4.0)],
        along: None,
    },
    PatternPreset {
        id: "teeth",
        name: "Teeth",
        formula: "height * (1 - abs(2 * x - 1))",
        vars: &[("height", 4.0)],
        along: None,
    },
    PatternPreset {
        id: "spikes",
        name: "Spikes",
        formula: "height * (1 - abs(2 * x - 1))^sharp",
        vars: &[("height", 4.0), ("sharp", 3.0)],
        along: None,
    },
    PatternPreset {
        id: "bumps",
        name: "Half circles",
        formula: "height * sqrt(max(0, 1 - (2 * x - 1)^2))",
        vars: &[("height", 4.0)],
        along: None,
    },
    PatternPreset {
        id: "stairs",
        name: "Stairs",
        formula: "height * min(steps, floor((1 - abs(2 * x - 1)) * (steps + 1))) / steps",
        vars: &[("height", 4.0), ("steps", 3.0)],
        along: None,
    },
    PatternPreset {
        id: "swell",
        name: "Swell",
        formula: "height * sin(x * pi) * sin(x * waves * 2 * pi)",
        vars: &[("height", 4.0), ("waves", 4.0)],
        along: None,
    },
    PatternPreset {
        id: "fade",
        name: "Fading wave",
        formula: "height * (1 - x) * sin(x * waves * 2 * pi)",
        vars: &[("height", 4.0), ("waves", 4.0)],
        along: None,
    },
    PatternPreset {
        id: "curls",
        name: "Curls",
        formula: "height * (1 - cos(x * 2 * pi)) / 2",
        vars: &[("height", 4.0), ("curl", 2.0)],
        along: Some("curl * sin(x * 2 * pi) / (2 * pi)"),
    },
];

/// 按 id 取预设
pub fn preset(id: &str) -> Option<&'static PatternPreset> {
    PATTERN_PRESETS.iter().find(|preset| preset.id == id)
}

/// 从预设构造参数；未知 id 返回 `None`
pub fn pattern_from_preset(id: &str, k: f32, loops: f32) -> Option<PatternParams> {
    let preset = preset(id)?;
    let vars = preset
        .vars
        .iter()
        .map(|(name, value)| ((*name).to_string(), *value))
        .collect::<BTreeMap<_, _>>();
    Some(PatternParams {
        preset: preset.id.to_string(),
        formula: preset.formula.to_string(),
        vars,
        along: preset.along.map(str::to_string),
        loops,
        each: false,
        k,
        mirror: false,
        scale: 1.0,
        sym: None::<Sym>,
        loop_curve: None,
    })
}

#[cfg(test)]
mod tests;
