//! 对话框窗口的逻辑尺寸（REF-001）。
//!
//! iced 视图层删除后不再有内容测量通道：尺寸完全由
//! [`dialog_preferred_size::dialog_size_hint`] 的静态声明决定（宽 = 声明值，
//! 高 = 声明下限，按工作区与声明上限夹取）。
//!
//! 方法名保留 `measure_dialog_window_size`：调用方（`lumino-dialog`）按尺寸
//! 「一次性决策」使用，与实现是否测量无关。

use iced_core::Size;

use crate::view::dialog_preferred_size;

use super::Host;

impl Host {
    /// 对话框窗口的推荐**逻辑尺寸**（静态声明表，不做内容测量）。
    ///
    /// 返回 `Size::ZERO` 表示声明非法（宽度非有限/非正），调用方必须回退种子
    /// 尺寸并**照常显示窗口**。
    pub fn measure_dialog_window_size(&self, work_area: Size) -> Size {
        dialog_preferred_size::static_window_size(
            dialog_preferred_size::dialog_size_hint(self.root.state.dialog_type),
            work_area,
        )
    }
}
