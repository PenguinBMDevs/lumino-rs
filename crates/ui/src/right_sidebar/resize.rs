//! 右侧栏面板宽度拖拽调整 — start/update/end_resize 与单元测试
//!
//! 拖拽锚点（`resize_start_x` / `resize_start_width`）必须由 `start_resize`
//! 在按下手柄时用**当前光标位置**初始化；否则 `update_resize_position` 会以
//! 初始值 `0.0 / DEFAULT_PANEL_WIDTH` 计算增量，右侧栏手柄位于窗口右端，
//! 增量深度为负导致面板瞬间回撤到最小宽度且无法再拉伸（2026-08-09 修复）。

use crate::right_sidebar::core::{MAX_PANEL_WIDTH, MIN_PANEL_WIDTH, RightSidebar};

impl RightSidebar {
    /// 开始拖拽调整面板宽度（记录按下位置与当前宽度作为增量锚点）
    pub fn start_resize(&mut self, cursor_x: f32) {
        self.is_resizing = true;
        self.resize_start_x = cursor_x;
        self.resize_start_width = self.panel_width;
    }

    /// 更新拖拽位置（面板宽度 = 锚点宽度 + 锚点 X - 当前 X）
    ///
    /// 右侧栏的拖拽方向与左侧相反：手柄在面板左缘，鼠标左移（X 减小）
    /// 时 `锚点 X - 当前 X` 为正 → 面板变宽。
    pub fn update_resize_position(&mut self, cursor_x: f32) {
        if self.is_resizing {
            let delta_x = self.resize_start_x - cursor_x;
            let new_width = self.resize_start_width + delta_x;
            self.panel_width = new_width.clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        }
    }

    /// 结束拖拽
    pub fn end_resize(&mut self) {
        self.is_resizing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 按下手柄：锚点 = 当前光标 X，基准宽度 = 当前面板宽度
    #[test]
    fn test_start_resize_records_anchor() {
        let mut sb = RightSidebar::new();
        sb.panel_width = 260.0;
        sb.start_resize(1030.0);
        assert!(sb.is_resizing);
        assert_eq!(sb.resize_start_x, 1030.0);
        assert_eq!(sb.resize_start_width, 260.0);
    }

    /// 鼠标左移（X 减小）→ 面板变宽（右侧栏方向与左侧栏相反）
    #[test]
    fn test_drag_left_widens_panel() {
        let mut sb = RightSidebar::new();
        sb.panel_width = 200.0;
        sb.start_resize(1030.0);
        sb.update_resize_position(1030.0 - 60.0);
        assert_eq!(sb.panel_width, 260.0);
    }

    /// 鼠标右移（X 增大）→ 面板变窄
    #[test]
    fn test_drag_right_narrows_panel() {
        let mut sb = RightSidebar::new();
        sb.panel_width = 300.0;
        sb.start_resize(1030.0);
        sb.update_resize_position(1030.0 + 80.0);
        assert_eq!(sb.panel_width, 220.0);
    }

    /// 未进入拖拽状态时移动鼠标不影响面板宽度
    #[test]
    fn test_update_ignored_when_not_resizing() {
        let mut sb = RightSidebar::new();
        sb.panel_width = 200.0;
        sb.update_resize_position(500.0);
        assert_eq!(sb.panel_width, 200.0);
    }

    /// 拖拽宽度下限 clamp（最小宽度）
    #[test]
    fn test_drag_clamps_to_min_width() {
        let mut sb = RightSidebar::new();
        sb.start_resize(200.0);
        // 向右拖出极大距离 → 面板宽度压向 MIN_PANEL_WIDTH
        sb.update_resize_position(10000.0);
        assert_eq!(sb.panel_width, MIN_PANEL_WIDTH);
    }

    /// 拖拽宽度上限 clamp（最大宽度）
    #[test]
    fn test_drag_clamps_to_max_width() {
        let mut sb = RightSidebar::new();
        sb.start_resize(1000.0);
        // 向左拖出极大距离 → 面板宽度撑向 MAX_PANEL_WIDTH
        sb.update_resize_position(0.0);
        assert_eq!(sb.panel_width, MAX_PANEL_WIDTH);
    }

    /// 结束拖拽后移动鼠标不再改变面板宽度
    #[test]
    fn test_end_resize_stops_updates() {
        let mut sb = RightSidebar::new();
        sb.start_resize(1030.0);
        sb.update_resize_position(1030.0 - 40.0);
        sb.end_resize();
        assert!(!sb.is_resizing);
        let frozen = sb.panel_width;
        sb.update_resize_position(0.0);
        assert_eq!(sb.panel_width, frozen);
    }

    /// 回归：第二次拖拽必须刷新锚点，而不是沿用上一次的起点
    #[test]
    fn test_second_drag_refreshes_anchor() {
        let mut sb = RightSidebar::new();
        // 第一次拖拽：从 1030 左移 60 → 260
        sb.start_resize(1030.0);
        sb.update_resize_position(970.0);
        sb.end_resize();
        assert_eq!(sb.panel_width, 260.0);
        // 第二次拖拽：手柄现在更靠左（约 260px 面板左缘），锚点重新记录
        sb.start_resize(760.0);
        sb.update_resize_position(760.0 + 100.0);
        assert_eq!(sb.panel_width, 160.0);
    }

    /// 回归：锚点必须由 start_resize 初始化，直接置 is_resizing 会以
    /// 初始值 0.0/200.0 计算增量（旧 BUG 路径：右侧栏手柄在窗口右端时
    /// 增量深度为负 → 面板瞬间回撤并卡死在最小宽度）。
    #[test]
    fn test_resize_requires_start_anchor() {
        // 旧 BUG 路径：仅标记 is_resizing，未调用 start_resize
        let mut buggy = RightSidebar::new();
        buggy.is_resizing = true;
        // 手柄实际位于窗口右端（例如 x ≈ 1032）
        buggy.update_resize_position(1032.0);
        assert!(buggy.panel_width >= MIN_PANEL_WIDTH);
        // 修复后正确路径：start_resize 刷新锚点后拖动即可恢复响应
        let mut fixed = RightSidebar::new();
        fixed.start_resize(1032.0);
        fixed.update_resize_position(1032.0 - 50.0);
        assert_eq!(fixed.panel_width, 200.0 + 50.0);
    }
}
