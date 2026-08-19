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
        /// 目标音轨索引。
        track_idx: u16,
        /// 自动化目标（参数标识）。
        target: AutomationTarget,
        /// MIDI 通道号（0-15）。
        channel: u8,
        /// 事件所在的 tick 位置。
        tick: u32,
        /// 自动化参数值。
        value: u16,
        /// 从此事件开始的线段插值形状。
        shape: SegmentShape,
    },
    /// 移动已有事件。
    Move {
        /// 目标音轨索引。
        track_idx: u16,
        /// 目标 lane 索引（按事件列表顺序）。
        lane_idx: usize,
        /// 原有 tick 位置。
        old_tick: u32,
        /// 旧值（精确匹配用）：弯音跳变对（同 tick 两事件）场景传
        /// `Some(原值)` 按 tick+value 定位目标；其他场景传 `None`
        /// 仅按 tick 匹配（同 tick 唯一）。
        old_value: Option<u16>,
        /// 移动后的新 tick 位置。
        new_tick: u32,
        /// 移动后的新参数值。
        new_value: u16,
    },
    /// 切换已有事件的 shape（双击）。
    CycleShape {
        /// 目标音轨索引。
        track_idx: u16,
        /// 目标 lane 索引（按事件列表顺序）。
        lane_idx: usize,
        /// 事件所在的 tick 位置。
        tick: u32,
    },
    /// 删除指定事件。
    Delete {
        /// 目标音轨索引。
        track_idx: u16,
        /// 目标 lane 索引（按事件列表顺序）。
        lane_idx: usize,
        /// 待删除事件所在的 tick 位置。
        tick: u32,
    },
    /// 更新已有事件的贝塞尔控制柄（实时拖柄用）。
    ///
    /// `handles_auto` 传入 `false`（拖柄 = 自定义柄）；如需恢复自动柄
    /// 由调用方用 `SetHandlesAuto`（当前未暴露，拖柄即标记自定义）。
    UpdateHandles {
        /// 目标音轨索引。
        track_idx: u16,
        /// 目标 lane 索引（按事件列表顺序）。
        lane_idx: usize,
        /// 事件所在的 tick 位置。
        tick: u32,
        /// 出向控制柄相对偏移 `(dtick, dvalue)`。
        out_handle: (f32, f32),
        /// 入向控制柄相对偏移 `(dtick, dvalue)`。
        in_handle: (f32, f32),
    },
    /// 清空指定 lane 的全部事件（√× 确认模式全量重建用）。
    Clear {
        /// 目标音轨索引。
        track_idx: u16,
        /// 目标 lane 索引（按事件列表顺序）。
        lane_idx: usize,
    },
}
