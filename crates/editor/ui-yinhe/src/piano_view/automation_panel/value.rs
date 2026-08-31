//! 自动化值域计算 — 对应 yinhe `automation_panel/value.rs`
//!
//! 复用 `lumino_note_core::AutomationTarget::max_value()`，不自建类型；
//! Tempo 通道在 lumino 侧由独立 `TempoPoint` 承载，值上限另行处理。

use super::types::AutomationPanelView;
use lumino_note_core::{AutomationLane, AutomationTarget};

/// 计算 `panel` 的值空间绝对上限（用于钳制 `value_zoom` 下限）。
///
/// - `show_velocity` → 127
/// - `Tempo` 在 lumino 侧无对应 `AutomationTarget`，调用方应走 `TempoPoint` 分支；
///   此处按 `max_value()` 兜底。
pub fn value_upper_bound(panel: &AutomationPanelView) -> f32 {
    if panel.show_velocity {
        127.0
    } else {
        panel.selected_target.max_value() as f32
    }
}

/// 面板当前 `target` 的显示上限。
///
/// - `show_velocity` → 127
/// - 其他 → `target.max_value()`（PitchBend=16383，CC=127，RPN 0/2=127 其余=16383）
/// - `tempo_lane` 仅当调用方显式传入 Tempo 面板时使用；`None` 时回退 `max_value()`。
#[must_use]
pub fn panel_max_val(panel: &AutomationPanelView, tempo_lane: Option<&AutomationLane>) -> f32 {
    if panel.show_velocity {
        return 127.0;
    }
    if panel.selected_target == AutomationTarget::PitchBend {
        return AutomationTarget::PitchBend.max_value() as f32;
    }
    if let Some(tl) = tempo_lane {
        if tl.target == AutomationTarget::PitchBend {
            // 约定：Tempo 走独立通道，此分支仅在调用方误传时触发
            return tl
                .events
                .iter()
                .map(|e| e.value as f32)
                .fold(0.0_f32, f32::max)
                .max(1.0);
        }
    }
    panel.selected_target.max_value() as f32
}

/// 便捷重载：无 Tempo lane 时的 `panel_max_val`。
#[must_use]
pub fn panel_max_val_simple(panel: &AutomationPanelView) -> f32 {
    panel_max_val(panel, None)
}

/// 计算 `value_zoom` 的下限，使得 `visible_range = max_val / zoom` 不超过 `upper_bound`。
#[must_use]
pub fn min_value_zoom(max_val: f32, upper_bound: f32) -> f32 {
    if upper_bound <= 0.0 {
        return 1.0;
    }
    (max_val / upper_bound).max(0.01)
}
