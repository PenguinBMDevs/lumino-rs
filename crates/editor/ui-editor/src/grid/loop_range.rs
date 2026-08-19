//! 循环区域数据模型

/// 循环区域状态
#[derive(Debug, Clone)]
pub struct LoopRange {
    enabled: bool,
    start_tick: f32,
    end_tick: f32,
    is_dragging_start: bool,
    is_dragging_end: bool,
    is_dragging_body: bool,
    drag_anchor_start_tick: f32,
    drag_anchor_mouse_tick: f32,
}

impl Default for LoopRange {
    fn default() -> Self {
        Self::new()
    }
}

impl LoopRange {
    const HANDLE_WIDTH: f32 = 6.0;
    const MIN_RANGE: f32 = 1.0;

    /// 创建一个默认（禁用）的循环区域实例。
    ///
    /// # 返回
    /// 范围固定在 `[0, 1920]` ticks 且未启用的 `LoopRange`。
    pub fn new() -> Self {
        Self {
            enabled: false,
            start_tick: 0.0,
            end_tick: 1920.0,
            is_dragging_start: false,
            is_dragging_end: false,
            is_dragging_body: false,
            drag_anchor_start_tick: 0.0,
            drag_anchor_mouse_tick: 0.0,
        }
    }

    /// 循环区域是否启用。
    ///
    /// # 返回
    /// 启用返回 `true`，否则返回 `false`。
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// 获取循环区域起始 tick。
    ///
    /// # 返回
    /// 起始 tick 值（浮点数）。
    pub fn start_tick(&self) -> f32 {
        self.start_tick
    }

    /// 获取循环区域结束 tick。
    ///
    /// # 返回
    /// 结束 tick 值（浮点数）。
    pub fn end_tick(&self) -> f32 {
        self.end_tick
    }

    /// 是否正在拖拽循环区域（起始句柄、结束句柄或整体）。
    ///
    /// # 返回
    /// 任一拖拽进行中返回 `true`。
    pub fn is_dragging(&self) -> bool {
        self.is_dragging_start || self.is_dragging_end || self.is_dragging_body
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        tracing::debug!("循环区域启用状态变更为: {}", enabled);
    }

    /// 设置循环区域范围（自动规范化顺序并限制最小范围）。
    ///
    /// # 参数
    /// * `start` — 起始 tick
    /// * `end` — 结束 tick
    pub fn set_range(&mut self, start: f32, end: f32) {
        let (s, e) = if start < end {
            (start, end)
        } else {
            (end, start)
        };
        let clamped_start = s.max(0.0);
        let clamped_end = e.max(clamped_start + Self::MIN_RANGE);
        self.start_tick = clamped_start;
        self.end_tick = clamped_end;
        tracing::debug!(
            "循环范围更新为 [{:.2}, {:.2}] ticks",
            self.start_tick,
            self.end_tick
        );
    }

    fn update_start(&mut self, tick: f32) {
        let clamped = tick.clamp(0.0, self.end_tick - Self::MIN_RANGE);
        if (self.start_tick - clamped).abs() > f32::EPSILON {
            self.start_tick = clamped;
            tracing::debug!("循环起始点更新为 {:.2} ticks", self.start_tick);
        }
    }

    fn update_end(&mut self, tick: f32) {
        let clamped = tick.max(self.start_tick + Self::MIN_RANGE);
        if (self.end_tick - clamped).abs() > f32::EPSILON {
            self.end_tick = clamped;
            tracing::debug!("循环结束点更新为 {:.2} ticks", self.end_tick);
        }
    }

    #[allow(dead_code)]
    fn move_range(&mut self, delta: f32) {
        let new_start = (self.start_tick + delta).max(0.0);
        let new_end = new_start + (self.end_tick - self.start_tick);
        self.set_range(new_start, new_end);
    }

    /// 指定 tick 是否包含在启用的循环区域内。
    ///
    /// # 参数
    /// * `tick` — 待检测的 tick 值
    ///
    /// # 返回
    /// 循环区域启用且 `tick` 落在 `[start, end]` 之间返回 `true`。
    pub fn contains(&self, tick: f32) -> bool {
        if !self.enabled {
            return false;
        }
        tick >= self.start_tick && tick <= self.end_tick
    }

    /// 获取循环区域长度（结束 tick 减起始 tick）。
    ///
    /// # 返回
    /// 区域的 tick 长度。
    pub fn length(&self) -> f32 {
        self.end_tick - self.start_tick
    }

    /// 切换循环区域的启用状态。
    pub fn toggle(&mut self) {
        self.set_enabled(!self.enabled);
    }

    /// 启用循环区域。
    pub fn enable(&mut self) {
        self.set_enabled(true);
    }

    /// 禁用循环区域。
    pub fn disable(&mut self) {
        self.set_enabled(false);
    }

    /// 处理标尺区域的鼠标按下事件，命中句柄/整体并进入对应拖拽状态。
    ///
    /// # 参数
    /// * `screen_x` — 鼠标的屏幕 X 坐标
    /// * `keyboard_width` — 键盘栏宽度（像素）
    /// * `scroll_x` — 水平滚动偏移
    /// * `zoom_x` — 水平缩放倍率
    /// * `_ruler_height` — 标尺高度（当前未使用）
    /// * `snap_precision` — 吸附精度（tick）
    ///
    /// # 返回
    /// 命中的拖拽目标类型（起始句柄/结束句柄/整体/无）。
    pub fn handle_mouse_press(
        &mut self,
        screen_x: f32,
        keyboard_width: f32,
        scroll_x: f32,
        zoom_x: f32,
        _ruler_height: f32,
        snap_precision: f32,
    ) -> LoopHitTest {
        if !self.enabled {
            return LoopHitTest::None;
        }

        let loop_screen_start = self.start_tick * zoom_x - scroll_x + keyboard_width;
        let loop_screen_end = self.end_tick * zoom_x - scroll_x + keyboard_width;

        if screen_x < loop_screen_start || screen_x > loop_screen_end {
            return LoopHitTest::None;
        }

        let handle_w = Self::HANDLE_WIDTH;

        if (screen_x - loop_screen_start).abs() <= handle_w / 2.0 {
            self.is_dragging_start = true;
            tracing::debug!("循环起始点拖拽开始");
            return LoopHitTest::StartHandle;
        }

        if (screen_x - loop_screen_end).abs() <= handle_w / 2.0 {
            self.is_dragging_end = true;
            tracing::debug!("循环结束点拖拽开始");
            return LoopHitTest::EndHandle;
        }

        self.is_dragging_body = true;
        let tick = (screen_x - keyboard_width + scroll_x) / zoom_x;
        let snapped = (tick / snap_precision).round() * snap_precision;
        self.drag_anchor_start_tick = self.start_tick;
        self.drag_anchor_mouse_tick = snapped;
        tracing::debug!("循环区域整体拖拽开始");
        LoopHitTest::Body
    }

    /// 处理拖拽过程中的鼠标移动，实时更新循环区域边界或整体位置。
    ///
    /// # 参数
    /// * `screen_x` — 鼠标的屏幕 X 坐标
    /// * `keyboard_width` — 键盘栏宽度（像素）
    /// * `scroll_x` — 水平滚动偏移
    /// * `zoom_x` — 水平缩放倍率
    /// * `snap_precision` — 吸附精度（tick）
    pub fn handle_mouse_move(
        &mut self,
        screen_x: f32,
        keyboard_width: f32,
        scroll_x: f32,
        zoom_x: f32,
        snap_precision: f32,
    ) {
        if self.is_dragging_start {
            let tick = (screen_x - keyboard_width + scroll_x) / zoom_x;
            let snapped = (tick / snap_precision).round() * snap_precision;
            self.update_start(snapped);
        } else if self.is_dragging_end {
            let tick = (screen_x - keyboard_width + scroll_x) / zoom_x;
            let snapped = (tick / snap_precision).round() * snap_precision;
            self.update_end(snapped);
        } else if self.is_dragging_body {
            let tick = (screen_x - keyboard_width + scroll_x) / zoom_x;
            let snapped = (tick / snap_precision).round() * snap_precision;
            let raw_delta = snapped - self.drag_anchor_mouse_tick;
            let delta = (raw_delta / snap_precision).round() * snap_precision;
            let new_start = (self.drag_anchor_start_tick + delta).max(0.0);
            let range_length = self.end_tick - self.start_tick;
            self.set_range(new_start, new_start + range_length);
        }
    }

    /// 结束循环区域拖拽，清除所有拖拽状态。
    pub fn handle_mouse_release(&mut self) {
        if self.is_dragging_start {
            tracing::debug!("循环起始点拖拽结束于 {:.2}", self.start_tick);
        }
        if self.is_dragging_end {
            tracing::debug!("循环结束点拖拽结束于 {:.2}", self.end_tick);
        }
        if self.is_dragging_body {
            tracing::debug!(
                "循环区域拖拽结束，范围为 [{:.2}, {:.2}]",
                self.start_tick,
                self.end_tick
            );
        }
        self.is_dragging_start = false;
        self.is_dragging_end = false;
        self.is_dragging_body = false;
    }

    /// 将循环区域转换为屏幕 X 坐标区间。
    ///
    /// # 参数
    /// * `keyboard_width` — 键盘栏宽度（像素）
    /// * `scroll_x` — 水平滚动偏移
    /// * `zoom_x` — 水平缩放倍率
    ///
    /// # 返回
    /// 启用时返回 `(start_screen_x, end_screen_x)`，禁用时返回 `None`。
    pub fn to_screen_coords(
        &self,
        keyboard_width: f32,
        scroll_x: f32,
        zoom_x: f32,
    ) -> Option<(f32, f32)> {
        if !self.enabled {
            return None;
        }
        let start = self.start_tick * zoom_x - scroll_x + keyboard_width;
        let end = self.end_tick * zoom_x - scroll_x + keyboard_width;
        Some((start, end))
    }

    /// 无副作用的命中检测（用于鼠标指针样式判断）
    pub fn hit_test_at(
        &self,
        screen_x: f32,
        keyboard_width: f32,
        scroll_x: f32,
        zoom_x: f32,
    ) -> LoopHitTest {
        if !self.enabled {
            return LoopHitTest::None;
        }
        let (loop_screen_start, loop_screen_end) =
            match self.to_screen_coords(keyboard_width, scroll_x, zoom_x) {
                Some(coords) => coords,
                None => return LoopHitTest::None,
            };

        if screen_x < loop_screen_start || screen_x > loop_screen_end {
            return LoopHitTest::None;
        }

        let handle_w = Self::HANDLE_WIDTH;
        if (screen_x - loop_screen_start).abs() <= handle_w / 2.0 {
            LoopHitTest::StartHandle
        } else if (screen_x - loop_screen_end).abs() <= handle_w / 2.0 {
            LoopHitTest::EndHandle
        } else {
            LoopHitTest::Body
        }
    }
}

/// 循环区域的命中检测结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopHitTest {
    /// 未命中循环区域
    None,
    /// 命中起始句柄
    StartHandle,
    /// 命中结束句柄
    EndHandle,
    /// 命中循环区域整体
    Body,
}

#[cfg(test)]
mod loop_range_tests;
