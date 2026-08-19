//! 自动化曲线渲染 — 从 AutomationLane 生成 GPU 实例
//!
//! 从 yinhe 项目移植：将 Step / Curve 插值的事件序列转换为 2px 线段与圆角锚点实例。

mod draw;
mod segments;

pub use segments::build_lane_instances;

/// 自动化节点（曲线 + 锚点）统一使用的蓝色，与主音轨已放置音符
/// `MAIN_TRACK_NOTE_COLOR`（ui crate note_worker.rs）保持一致，确保视觉统一。
pub const AUTOMATION_NODE_COLOR: [f32; 3] = [0.2, 0.55, 1.0];

/// 自动化面板局部视图参数（与 yinhe 的 AutomationPanelView 对应的最小集）。
#[derive(Debug, Clone, Copy)]
pub struct AutomationViewParams {
    /// 面板高度（像素）
    pub panel_height: f32,
    /// 每 tick 对应的像素数
    pub pixels_per_tick: f32,
    /// 水平滚动偏移（像素）
    pub scroll_x: f32,
    /// 左侧键盘/轨道列宽度（像素）
    pub keyboard_width: f32,
    /// 垂直缩放系数。1.0 = 满量程映射到面板高度。
    pub value_zoom: f32,
    /// 垂直滚动偏移（值空间单位）。面板顶部对应的值。
    pub value_scroll: f32,
    /// 面板内容区左上角屏幕 X 坐标
    pub panel_offset_x: f32,
    /// 面板内容区左上角屏幕 Y 坐标
    pub panel_offset_y: f32,
    /// 工具栏高度（像素），数据区在工具栏下方
    pub toolbar_height: f32,
    /// 自动化曲线连线粗细（像素，1-10，默认 2）。
    pub line_thickness: f32,
}

impl AutomationViewParams {
    /// 将 tick 转换为屏幕空间 X 坐标（含滚动、键盘宽度与面板偏移）。
    #[inline]
    pub fn tick_to_x(&self, tick: u32) -> f32 {
        self.panel_offset_x + self.keyboard_width - self.scroll_x
            + tick as f32 * self.pixels_per_tick
    }

    /// 将自动化值转换为屏幕空间 Y 坐标（像素）。
    #[inline]
    pub fn value_to_y(&self, value: f32, max_val: f32) -> f32 {
        let visible_range = max_val / self.value_zoom;
        if visible_range <= 0.0 {
            return self.panel_offset_y + self.toolbar_height;
        }
        let available_height = self.panel_height - self.toolbar_height;
        self.panel_offset_y + self.toolbar_height + available_height
            - ((value - self.value_scroll) / visible_range) * available_height
    }

    /// 将屏幕空间 Y 坐标转换回自动化值。
    #[inline]
    pub fn y_to_value(&self, y: f32, max_val: f32) -> f32 {
        let visible_range = max_val / self.value_zoom;
        if visible_range <= 0.0 {
            return 0.0;
        }
        let available_height = self.panel_height - self.toolbar_height;
        let local_y = y - self.panel_offset_y - self.toolbar_height;
        self.value_scroll + (1.0 - local_y / available_height) * visible_range
    }

    /// 根据 max_val 限制 value_scroll 的范围。
    pub fn clamp_value_scroll(&mut self, max_val: f32) {
        let visible_range = max_val / self.value_zoom;
        let max_scroll = (max_val - visible_range).max(0.0);
        self.value_scroll = self.value_scroll.clamp(0.0, max_scroll);
    }
}

#[cfg(test)]
mod tests;
