//! 事件浏览器类型 ↔ 自动化存储类型转换
//!
//! - 目标映射：事件浏览器 `AutomationTarget` ↔ 存储 lane `AutomationTarget`
//! - 形状映射：事件浏览器贝塞尔控制点 ↔ 存储 tension
//!
//! 正向映射（tension → 控制点）在 UI 层 `detail/auto.rs::lane_shape_to_event_shape`，
//! 本模块的反向转换与其保持互逆（见 `curve_to_tension`）。

use lumino_note_core::automation as auto;
use lumino_note_core::event::{AutomationTarget, SegmentShape};

/// 将事件浏览器自动化目标映射到现有自动化 lane 目标。
///
/// Tempo 不映射（单独存于 tempo_points），调用方必须先行处理。
pub(super) fn event_target_to_auto_target(target: &AutomationTarget) -> auto::AutomationTarget {
    match target {
        AutomationTarget::Cc(controller) => auto::AutomationTarget::CC {
            controller: *controller,
        },
        AutomationTarget::PitchBend => auto::AutomationTarget::PitchBend,
        AutomationTarget::Rpn(parameter) => auto::AutomationTarget::Rpn {
            parameter: *parameter,
        },
        AutomationTarget::Nrpn(parameter) => auto::AutomationTarget::Nrpn {
            parameter: *parameter,
        },
        AutomationTarget::Tempo => unreachable!("Tempo 目标应单独处理，不映射到自动化 lane"),
    }
}

/// 将事件浏览器线段形状映射到现有自动化 lane 形状。
///
/// Step 直通；Curve 控制点通过贝塞尔中点反算为 tension（有损，见
/// `curve_to_tension`）。
pub(super) fn event_shape_to_auto_shape(shape: SegmentShape) -> auto::SegmentShape {
    match shape {
        SegmentShape::Step => auto::SegmentShape::Step,
        SegmentShape::Curve { x1, y1, x2, y2 } => auto::SegmentShape::Curve {
            tension: curve_to_tension(x1, y1, x2, y2),
        },
    }
}

/// 贝塞尔控制点 → tension（-127..127）。
///
/// 正向映射（`lane_shape_to_event_shape`）把 tension 编码为：
/// - `t >= 0`：`y1 = 0`，`y2 = 0.5t`
/// - `t < 0`：`y1 = -0.5t`，`y2 = 1`
///
/// 反向优先匹配上述流形（精确互逆）；任意控制点则用贝塞尔中点
/// `m = (3y1 + 3y2 + 1) / 8`（线性段 m=0.5）反推 t，再映射到 [-127, 127]。
/// x 控制点不参与 tension 编码（显示时固定 x1=0.25, x2=0.75）。
pub(super) fn curve_to_tension(x1: f32, y1: f32, x2: f32, y2: f32) -> i8 {
    let _ = (x1, x2); // x 控制点只影响时间分布，tension 模型不编码
    const EPS: f32 = 0.02;
    let t = if y1.abs() <= EPS && (y2 - 1.0).abs() <= EPS {
        // 直线段（t = 0 流形）
        0.0
    } else if y1.abs() <= EPS {
        // ease-in 流形：y2 = 0.5t → t = 2*y2
        2.0 * y2
    } else if (y2 - 1.0).abs() <= EPS {
        // ease-out 流形：y1 = -0.5t → t = -2*y1
        -2.0 * y1
    } else {
        // 通用：贝塞尔中点 m，线性映射 [0.3125, 0.6875] → [+1, -1]
        let m = (3.0 * y1 + 3.0 * y2 + 1.0) / 8.0;
        (0.5 - m) / 0.1875
    };
    (t.clamp(-1.0, 1.0) * 127.0).round() as i8
}
