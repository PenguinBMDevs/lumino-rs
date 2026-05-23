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
    ) {
        if self.is_dragging_start {
            let tick = (screen_x - keyboard_width + scroll_x) / zoom_x;
            self.update_start(tick);
        } else if self.is_dragging_end {
            let tick = (screen_x - keyboard_width + scroll_x) / zoom_x;
            self.update_end(tick);
        } else if self.is_dragging_body {
            let tick = (screen_x - keyboard_width + scroll_x) / zoom_x;
            let center = (self.start_tick + self.end_tick) / 2.0;
            let delta = tick - center;
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
}
