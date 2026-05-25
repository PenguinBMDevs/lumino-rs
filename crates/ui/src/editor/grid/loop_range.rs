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
}

impl Default for LoopRange {
    fn default() -> Self {
        Self::new()
    }
}

impl LoopRange {
    const HANDLE_WIDTH: f32 = 6.0;
    const MIN_RANGE: f32 = 1.0;

    pub fn new() -> Self {
        Self {
            enabled: false,
            start_tick: 0.0,
            end_tick: 1920.0,
            is_dragging_start: false,
            is_dragging_end: false,
            is_dragging_body: false,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn start_tick(&self) -> f32 {
        self.start_tick
    }

    pub fn end_tick(&self) -> f32 {
        self.end_tick
    }

    pub fn is_dragging(&self) -> bool {
        self.is_dragging_start || self.is_dragging_end || self.is_dragging_body
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        tracing::debug!("循环区域启用状态变更为: {}", enabled);
    }

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

    fn move_range(&mut self, delta: f32) {
        let new_start = (self.start_tick + delta).max(0.0);
        let new_end = new_start + (self.end_tick - self.start_tick);
        self.set_range(new_start, new_end);
    }

    pub fn contains(&self, tick: f32) -> bool {
        if !self.enabled {
            return false;
        }
        tick >= self.start_tick && tick <= self.end_tick
    }

    pub fn length(&self) -> f32 {
        self.end_tick - self.start_tick
    }

    pub fn toggle(&mut self) {
        self.set_enabled(!self.enabled);
    }

    pub fn enable(&mut self) {
        self.set_enabled(true);
    }

    pub fn disable(&mut self) {
        self.set_enabled(false);
    }

    pub fn handle_mouse_press(
        &mut self,
        screen_x: f32,
        keyboard_width: f32,
        scroll_x: f32,
        zoom_x: f32,
        _ruler_height: f32,
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
        tracing::debug!("循环区域整体拖拽开始");
        LoopHitTest::Body
    }

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
            let center = (self.start_tick + self.end_tick) / 2.0;
            let rough_delta = snapped - center;
            let delta = (rough_delta / snap_precision).round() * snap_precision;
            self.move_range(delta);
        }
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopHitTest {
    None,
    StartHandle,
    EndHandle,
    Body,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_range_creation() {
        let loop_range = LoopRange::new();
        assert!(!loop_range.enabled());
        assert!((loop_range.start_tick() - 0.0).abs() < f32::EPSILON);
        assert!((loop_range.end_tick() - 1920.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_range_toggle() {
        let mut loop_range = LoopRange::new();
        assert!(!loop_range.enabled());
        loop_range.toggle();
        assert!(loop_range.enabled());
        loop_range.toggle();
        assert!(!loop_range.enabled());
    }

    #[test]
    fn test_loop_range_enable_disable() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();
        assert!(loop_range.enabled());
        loop_range.disable();
        assert!(!loop_range.enabled());
    }

    #[test]
    fn test_loop_range_resize() {
        let mut loop_range = LoopRange::new();
        loop_range.set_range(100.0, 500.0);
        assert!((loop_range.start_tick() - 100.0).abs() < f32::EPSILON);
        assert!((loop_range.end_tick() - 500.0).abs() < f32::EPSILON);

        loop_range.update_start(200.0);
        assert!((loop_range.start_tick() - 200.0).abs() < f32::EPSILON);

        loop_range.update_end(800.0);
        assert!((loop_range.end_tick() - 800.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_contains_tick() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();

        loop_range.set_range(100.0, 500.0);

        assert!(!loop_range.contains(50.0));
        assert!(loop_range.contains(100.0));
        assert!(loop_range.contains(300.0));
        assert!(loop_range.contains(500.0));
        assert!(!loop_range.contains(600.0));

        loop_range.disable();
        assert!(!loop_range.contains(300.0));
    }

    #[test]
    fn test_loop_boundary_conditions() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();

        loop_range.set_range(-10.0, -5.0);
        assert!(loop_range.start_tick() >= 0.0);
        assert!(loop_range.end_tick() > loop_range.start_tick());

        loop_range.update_start(10000.0);
        assert!(loop_range.end_tick() > loop_range.start_tick());

        loop_range.update_end(-100.0);
        assert!(loop_range.end_tick() > loop_range.start_tick());

        loop_range.set_range(100.0, 50.0);
        assert!(loop_range.start_tick() <= loop_range.end_tick());
    }

    #[test]
    fn test_loop_length() {
        let mut loop_range = LoopRange::new();
        loop_range.set_range(100.0, 500.0);
        assert!((loop_range.length() - 400.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_to_screen_coords() {
        let mut loop_range = LoopRange::new();
        assert!(loop_range.to_screen_coords(200.0, 0.0, 1.0).is_none());

        loop_range.enable();
        loop_range.set_range(100.0, 500.0);

        let coords = loop_range.to_screen_coords(200.0, 0.0, 1.0);
        assert!(coords.is_some());
        let (start, end) = coords.unwrap();
        assert!((start - 300.0).abs() < f32::EPSILON);
        assert!((end - 700.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_move_range() {
        let mut loop_range = LoopRange::new();
        loop_range.set_range(100.0, 500.0);
        loop_range.move_range(50.0);
        assert!((loop_range.start_tick() - 150.0).abs() < f32::EPSILON);
        assert!((loop_range.end_tick() - 550.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_move_range_negative() {
        let mut loop_range = LoopRange::new();
        loop_range.set_range(100.0, 500.0);
        loop_range.move_range(-50.0);
        assert!((loop_range.start_tick() - 50.0).abs() < f32::EPSILON);
        assert!((loop_range.end_tick() - 450.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_loop_is_dragging() {
        let mut loop_range = LoopRange::new();
        assert!(!loop_range.is_dragging());
        loop_range.is_dragging_start = true;
        assert!(loop_range.is_dragging());
        loop_range.is_dragging_start = false;
        loop_range.is_dragging_end = true;
        assert!(loop_range.is_dragging());
        loop_range.is_dragging_end = false;
        loop_range.is_dragging_body = true;
        assert!(loop_range.is_dragging());
    }

    #[test]
    fn test_default_implementation() {
        let loop_range = LoopRange::default();
        assert!(!loop_range.enabled());
        assert!((loop_range.start_tick() - 0.0).abs() < f32::EPSILON);
        assert!((loop_range.end_tick() - 1920.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_body_drag_delta_aligned_to_snap_precision() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();
        loop_range.set_range(0.0, 1920.0);
        // 初始 center = 960.0

        let snap_precision = 1920.0; // quarter note at PPQ=1920

        // 模拟 body drag 从鼠标位置转换所得 tick = 1500 (first drag, near start)
        loop_range.is_dragging_body = true;
        loop_range.handle_mouse_move(1500.0, 0.0, 0.0, 1.0, snap_precision);
        // delta = round((1920-960)/1920)*1920 = round(960/1920)*1920 = 1920
        // [0,1920] -> [1920,3840]
        assert!(
            (loop_range.start_tick() - 1920.0).abs() < f32::EPSILON,
            "第一次拖动 delta 应为 1920(四分音符)，实际 start_tick={}",
            loop_range.start_tick()
        );
        assert!((loop_range.end_tick() - 3840.0).abs() < f32::EPSILON);

        // 第二次拖动: center = 2880, mouse tick = 5000
        loop_range.handle_mouse_move(5000.0, 0.0, 0.0, 1.0, snap_precision);
        // snapped = round(5000/1920)*1920 = 5760
        // center = 2880, rough_delta = 2880, round(2880/1920)*1920 = 3840
        // [1920,3840] -> [5760,7680]
        assert!(
            (loop_range.start_tick() - 5760.0).abs() < f32::EPSILON,
            "第二次拖动 delta 应为 3840(2倍四分音符)，实际 start_tick={}",
            loop_range.start_tick()
        );
        assert!((loop_range.end_tick() - 7680.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_body_drag_precision_with_ppq_480() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();
        loop_range.set_range(0.0, 1920.0);
        // 初始 center = 960.0

        let snap_precision = 480.0; // quarter note at PPQ=480

        // 模拟 body drag，第一次有效移动
        loop_range.is_dragging_body = true;
        loop_range.handle_mouse_move(1500.0, 0.0, 0.0, 1.0, snap_precision);
        // tick=1500, snapped=1440, center=960, rough_delta=480, delta=480
        // [0,1920] -> [480,2400]
        let start1 = loop_range.start_tick();
        assert!(
            (start1 - 480.0).abs() < f32::EPSILON,
            "PPQ=480 第一次 delta 应为 480(四分音符), 实际 start_tick={}",
            start1
        );

        // 继续拖动: center = 1440, mouse tick = 2500
        loop_range.handle_mouse_move(2500.0, 0.0, 0.0, 1.0, snap_precision);
        // tick=2500, snapped=2400, center=1440, rough_delta=960, delta=960
        // [480,2400] -> [1440,3360]
        let start2 = loop_range.start_tick();
        assert!(
            (start2 - 1440.0).abs() < f32::EPSILON,
            "PPQ=480 第二次 delta 应为 960, 实际 start_tick={}",
            start2
        );
    }

    #[test]
    fn test_body_drag_delta_always_multiple_of_snap_precision() {
        let mut loop_range = LoopRange::new();
        loop_range.enable();

        // 用非标准中心位置测试
        loop_range.set_range(100.0, 500.0);
        // center = 300.0

        let snap_precision = 120.0;

        loop_range.is_dragging_body = true;
        // 随机拖拽位置
        for mouse_tick in [350.0, 600.0, 777.0, 1200.0, 2500.0] {
            loop_range.handle_mouse_move(mouse_tick, 0.0, 0.0, 1.0, snap_precision);
            let center = loop_range.start_tick() + loop_range.length() / 2.0;
            let delta = center - 300.0;
            let snapped_delta = (delta / snap_precision).round() * snap_precision;
            assert!(
                (delta - snapped_delta).abs() < 1e-4,
                "delta {} 应为 {} 的整数倍, 偏差={}",
                delta,
                snap_precision,
                (delta - snapped_delta).abs()
            );
        }
    }
}
