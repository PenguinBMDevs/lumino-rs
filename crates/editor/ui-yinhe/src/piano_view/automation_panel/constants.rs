//! 自动化面板常量 — 对应 yinhe `automation_panel/constants.rs`
//!
//! 复用 `lumino_note_core::AutomationTarget`，不自建类型；
//! 数值与 yinhe 保持一致，便于交互/渲染层对齐。

use lumino_note_core::AutomationTarget;

/// 面板下拉中可直接选择的已知自动化目标（与 yinhe `AUTOMATION_TARGETS` 对齐）。
///
/// lumino 侧 `AutomationTarget` 无 `Tempo` variant（Tempo 由独立 `TempoPoint` 通道承载），
/// 此处仅列 CC / PB / RPN；需要 Tempo 面板时由调用方按 `show_velocity = false + TempoPoint`
/// 分支单独承载。
pub const AUTOMATION_TARGETS: &[AutomationTarget] = &[
    AutomationTarget::CC { controller: 7 },  // Volume
    AutomationTarget::CC { controller: 10 }, // Pan
    AutomationTarget::CC { controller: 11 }, // Expression
    AutomationTarget::CC { controller: 64 }, // Sustain
    AutomationTarget::CC { controller: 71 }, // Resonance
    AutomationTarget::CC { controller: 72 }, // Release
    AutomationTarget::CC { controller: 73 }, // Attack
    AutomationTarget::CC { controller: 74 }, // Cutoff
    AutomationTarget::PitchBend,
    AutomationTarget::Rpn { parameter: 0 }, // PB Sensitivity
    AutomationTarget::Rpn { parameter: 1 }, // Fine Tune
    AutomationTarget::Rpn { parameter: 2 }, // Coarse Tune
];

/// 锚点命中半径（像素），与 yinhe `ANCHOR_HIT_PX = 10.0` 一致。
pub const ANCHOR_HIT_PX: f32 = 10.0;

/// 面板间分割条高度（像素），与 yinhe `SPLIT_H` 一致。
pub const SPLIT_H: f32 = 6.0;

/// 悬停多久后显示 tooltip（秒），与 yinhe `HOVER_DELAY = 0.6` 一致。
pub const HOVER_DELAY: f64 = 0.6;

/// 选框拖拽最小触发距离（像素），小于此视为点击，不触发选区清空。
pub const MARQUEE_THRESHOLD: f32 = 3.0;

/// 面板最小/默认/最大高度（像素），与 yinhe `automation_panel_view.rs` 一致。
pub const MIN_PANEL_HEIGHT: f32 = 40.0;
pub const DEFAULT_PANEL_HEIGHT: f32 = 80.0;
pub const MAX_PANEL_HEIGHT: f32 = 200.0;

/// 速度/弯音等数值面板的垂直内边距与手柄高度（与 `gfx::cc_bar_renderer::prepare` 对齐）。
pub const PANEL_PADDING_Y: f32 = 12.0;
pub const RESIZE_HANDLE_HEIGHT: f32 = 5.0;
