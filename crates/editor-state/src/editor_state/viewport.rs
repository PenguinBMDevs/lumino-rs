//! 视口操作逻辑
//!
//! 将滚动、缩放、可见键数量等视图相关计算从 `EditorState` facade 中提取出来，
//! 通过 `Viewport` 结构统一管理 `ViewState` 和 `max_scroll` 的联动。

use lumino_core::view_state::ViewState;

/// 视口封装，持有 `ViewState` 和 `max_scroll` 的可变引用。
///
/// 所有会同时修改 `ViewState` 和 `max_scroll` 的操作都通过此结构进行，
/// 避免在 `EditorState` 中重复书写联动逻辑。
pub struct Viewport<'a> {
    pub view: &'a mut ViewState,
    pub max_scroll: &'a mut (f32, f32),
}

impl<'a> Viewport<'a> {
    /// 创建新的视口封装
    pub fn new(view: &'a mut ViewState, max_scroll: &'a mut (f32, f32)) -> Self {
        Self { view, max_scroll }
    }

    /// 根据总 tick 数更新最大滚动范围
    pub fn update_max_scroll(&mut self, total_ticks: u32) {
        *self.max_scroll = (
            total_ticks as f32 * self.view.zoom_x,
            self.view.visible_key_count as f32 * self.view.zoom_y,
        );
    }

    /// 设置水平滚动位置
    pub fn set_scroll_x(&mut self, scroll_x: f32, keyboard_width: f32, canvas_width: f32) {
        let total_width = self.view.total_ticks as f32 * self.view.zoom_x;
        let viewport_width = (canvas_width - keyboard_width).max(0.0);
        let max_scroll = (total_width - viewport_width).max(0.0);
        self.view.scroll_x = scroll_x.max(0.0).min(max_scroll);
        self.view.smooth_scroll.target_x = self.view.scroll_x;
        self.view.smooth_scroll.active = false;
    }

    /// 设置垂直滚动位置
    pub fn set_scroll_y(&mut self, scroll_y: f32, canvas_height: f32) {
        let total_height = self.view.visible_key_count as f32 * self.view.zoom_y;
        let viewport_height = (canvas_height - self.view.ruler_height).max(0.0);
        let max_scroll = (total_height - viewport_height).max(0.0);
        self.view.scroll_y = scroll_y.max(0.0).min(max_scroll);
        self.view.smooth_scroll.target_y = self.view.scroll_y;
        self.view.smooth_scroll.active = false;
    }

    /// 设置水平缩放
    pub fn set_zoom_x(
        &mut self,
        zoom_x: f32,
        fixed_ratio: f32,
        keyboard_width: f32,
        canvas_width: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) {
        let old = self.view.zoom_x;
        self.view.zoom_x = zoom_x.clamp(min_zoom, max_zoom);
        let ratio = self.view.zoom_x / old;
        let viewport_width = (canvas_width - keyboard_width).max(0.0);
        let fixed_point = self.view.scroll_x + viewport_width * fixed_ratio;
        self.view.scroll_x = fixed_point * ratio - viewport_width * fixed_ratio;
        self.update_max_scroll(self.view.total_ticks);
        let max_scroll = (self.max_scroll.0 - viewport_width).max(0.0);
        self.view.scroll_x = self.view.scroll_x.max(0.0).min(max_scroll);
    }

    /// 设置垂直缩放
    pub fn set_zoom_y(
        &mut self,
        zoom_y: f32,
        fixed_ratio: f32,
        canvas_height: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) {
        let old = self.view.zoom_y;
        self.view.zoom_y = zoom_y.clamp(min_zoom, max_zoom);
        let ratio = self.view.zoom_y / old;
        let viewport_height = canvas_height.max(0.0);
        let fixed_point = self.view.scroll_y + viewport_height * fixed_ratio;
        self.view.scroll_y = fixed_point * ratio - viewport_height * fixed_ratio;
        self.update_max_scroll(self.view.total_ticks);
        let viewport_height2 = (canvas_height - self.view.ruler_height).max(0.0);
        let max_scroll = (self.max_scroll.1 - viewport_height2).max(0.0);
        self.view.scroll_y = self.view.scroll_y.max(0.0).min(max_scroll);
    }

    /// 设置可见键数量
    pub fn set_visible_key_count(
        &mut self,
        count: u16,
        min_count: u16,
        max_count: u16,
        canvas_height: f32,
    ) {
        self.view.visible_key_count = count.clamp(min_count, max_count);
        self.update_max_scroll(self.view.total_ticks);
        let viewport_height = (canvas_height - self.view.ruler_height).max(0.0);
        let max_scroll = (self.max_scroll.1 - viewport_height).max(0.0);
        if self.view.scroll_y > max_scroll {
            self.view.scroll_y = max_scroll;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_viewport() -> (ViewState, (f32, f32)) {
        (ViewState::default(), (0.0, 0.0))
    }

    #[test]
    fn test_update_max_scroll() {
        let (mut view, mut max_scroll) = setup_viewport();
        let mut vp = Viewport::new(&mut view, &mut max_scroll);
        vp.update_max_scroll(1000);
        assert!(max_scroll.0 > 0.0);
        assert!(max_scroll.1 > 0.0);
    }

    #[test]
    fn test_set_scroll_x_clamps_to_max() {
        let (mut view, mut max_scroll) = setup_viewport();
        let total_ticks = view.total_ticks;
        {
            let mut vp = Viewport::new(&mut view, &mut max_scroll);
            vp.update_max_scroll(total_ticks);
            vp.set_scroll_x(100000.0, 120.0, 800.0);
        }
        assert!(view.scroll_x <= max_scroll.0);
    }

    #[test]
    fn test_set_scroll_x_clamps_to_zero() {
        let (mut view, mut max_scroll) = setup_viewport();
        {
            let mut vp = Viewport::new(&mut view, &mut max_scroll);
            vp.set_scroll_x(-100.0, 120.0, 800.0);
        }
        assert_eq!(view.scroll_x, 0.0);
    }

    #[test]
    fn test_set_zoom_x_updates_max_scroll() {
        let (mut view, mut max_scroll) = setup_viewport();
        let total_ticks = view.total_ticks;
        {
            let mut vp = Viewport::new(&mut view, &mut max_scroll);
            vp.update_max_scroll(total_ticks);
        }
        let old_max_x = max_scroll.0;
        {
            let mut vp = Viewport::new(&mut view, &mut max_scroll);
            vp.set_zoom_x(0.5, 0.5, 120.0, 800.0, 0.01, 10.0);
        }
        assert_ne!(max_scroll.0, old_max_x);
    }

    #[test]
    fn test_set_visible_key_count_clamps_count() {
        let (mut view, mut max_scroll) = setup_viewport();
        let mut vp = Viewport::new(&mut view, &mut max_scroll);
        vp.set_visible_key_count(10, 20, 100, 800.0);
        assert_eq!(vp.view.visible_key_count, 20);
    }
}
