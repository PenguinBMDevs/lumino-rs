//! 自动化编辑操作类型
//!
//! `AutomationEdit` 由 UI 交互层产生，应用到 `EditorData` 的自动化数据。
//! 从 `automation.rs` 拆分而来。

use super::{AutomationTarget, SegmentShape};

/// 自动化编辑操作。
///
/// 由 UI 交互层产生，应用到 `EditorData` 的自动化数据。
#[derive(Clone, Debug)]
pub enum AutomationEdit {
    /// 添加新事件。若 lane 不存在则自动创建。
    Add {
        track_idx: u16,
        target: AutomationTarget,
        /// MIDI 通道号（0-15）。
        channel: u8,
        tick: u32,
        value: u16,
        shape: SegmentShape,
    },
    /// 移动已有事件。
    Move {
        track_idx: u16,
        lane_idx: usize,
        old_tick: u32,
        /// 旧值（精确匹配用）：弯音跳变对（同 tick 两事件）场景传
        /// `Some(原值)` 按 tick+value 定位目标；其他场景传 `None`
        /// 仅按 tick 匹配（同 tick 唯一）。
        old_value: Option<u16>,
        new_tick: u32,
        new_value: u16,
    },
    /// 切换已有事件的 shape（双击）。
    CycleShape {
        track_idx: u16,
        lane_idx: usize,
        tick: u32,
    },
    /// 删除指定事件。
    Delete {
        track_idx: u16,
        lane_idx: usize,
        tick: u32,
    },
    /// 更新已有事件的贝塞尔控制柄（实时拖柄用）。
    ///
    /// `handles_auto` 传入 `false`（拖柄 = 自定义柄）；如需恢复自动柄
    /// 由调用方用 `SetHandlesAuto`（当前未暴露，拖柄即标记自定义）。
    UpdateHandles {
        track_idx: u16,
        lane_idx: usize,
        tick: u32,
        out_handle: (f32, f32),
        in_handle: (f32, f32),
    },
    /// 清空指定 lane 的全部事件（√× 确认模式全量重建用）。
    Clear { track_idx: u16, lane_idx: usize },
}
