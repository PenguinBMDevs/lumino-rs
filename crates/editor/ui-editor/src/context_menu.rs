//! 钢琴卷帘右键上下文菜单状态。
//!
//! 视图已迁到 egui（`lumino-ui-egui::piano_roll_context_menu`）：
//! 条目集与 tooltip 文案由 `lumino_message::context_menu` 单一来源提供，
//! iced 侧只保留「是否打开 / 打开位置 / 作用目标」的状态，输入仍由
//! iced Canvas program 产生（`Message::PianoRollContextMenu(Open)`）。

use iced_core::Point;
use lumino_message::ContextMenuTarget;

/// 钢琴卷帘右键菜单状态
#[derive(Debug, Clone, Default)]
pub struct PianoRollContextMenuState {
    /// 菜单是否打开
    pub open: bool,
    /// 菜单弹出位置（canvas 局部坐标）
    pub position: Option<Point>,
    /// 菜单作用目标（决定渲染哪些条目）
    pub target: ContextMenuTarget,
}

impl PianoRollContextMenuState {
    /// 打开菜单（指定位置与目标）
    pub fn open(&mut self, position: Point, target: ContextMenuTarget) {
        self.open = true;
        self.position = Some(position);
        self.target = target;
    }

    /// 关闭菜单
    pub fn close(&mut self) {
        self.open = false;
        self.position = None;
    }

    /// 切换菜单状态
    #[allow(dead_code)]
    pub fn toggle(&mut self, position: Point, target: ContextMenuTarget) {
        if self.open {
            self.close();
        } else {
            self.open(position, target);
        }
    }
}

#[cfg(test)]
mod tests;
