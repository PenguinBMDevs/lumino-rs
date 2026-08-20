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
    /// 可变引用的视图状态
    pub view: &'a mut ViewState,
    /// 可变引用的最大滚动范围（横向、纵向）
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
    ///
    /// `time_zoom` 为时间轴的像素缩放（横向为 `zoom_x`；纵向卷帘转置后时间轴在 Y 方向，传 `zoom_y`）。
    /// `keyboard_width` 为 pitch 轴方向的留白尺寸（纵向卷帘传键盘高度）。
    pub fn set_scroll_x(
        &mut self,
        scroll_x: f32,
        keyboard_width: f32,
        canvas_width: f32,
        time_zoom: f32,
    ) {
        let total_width = self.view.total_ticks as f32 * time_zoom;
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
        // 缩放是精确视图操作：终止遗留的平滑滚动动画，避免动画继续把
        // scroll_x 拉向旧目标、覆盖缩放锚点补偿（表现为"Ctrl+滚轮缩放时
        // 卷帘还在左右滚动"）。与 set_scroll_x 的动画终止语义保持一致。
        self.view.smooth_scroll.target_x = self.view.scroll_x;
        self.view.smooth_scroll.active = false;
    }

    /// 设置垂直缩放
    ///
    /// `fixed_ratio` 语义：鼠标在**内容区**（canvas 高度减去标尺区）内的锚点比例，
    /// 0.0 贴内容区顶部、1.0 贴底部（与 `set_zoom_x` 的 `width - keyboard_width` 对齐）。
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
        let viewport_height = (canvas_height - self.view.ruler_height).max(0.0);
        let fixed_point = self.view.scroll_y + viewport_height * fixed_ratio;
        self.view.scroll_y = fixed_point * ratio - viewport_height * fixed_ratio;
        self.update_max_scroll(self.view.total_ticks);
        let max_scroll = (self.max_scroll.1 - viewport_height).max(0.0);
        self.view.scroll_y = self.view.scroll_y.max(0.0).min(max_scroll);
        // 缩放是精确视图操作：终止遗留的平滑滚动动画，避免动画继续把
        // scroll_y 拉向旧目标、覆盖缩放锚点补偿。与 set_scroll_y 一致。
        self.view.smooth_scroll.target_y = self.view.scroll_y;
        self.view.smooth_scroll.active = false;
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
        let time_zoom = 1.0;
        let keyboard_width = 120.0;
        let canvas_width = 800.0;
        let expected_max = (total_ticks as f32 * time_zoom
            - f32::max(canvas_width - keyboard_width, 0.0))
        .max(0.0);
        {
            let mut vp = Viewport::new(&mut view, &mut max_scroll);
            vp.update_max_scroll(total_ticks);
            vp.set_scroll_x(100000.0, keyboard_width, canvas_width, time_zoom);
        }
        assert!(view.scroll_x <= expected_max);
    }

    #[test]
    fn test_set_scroll_x_clamps_to_zero() {
        let (mut view, mut max_scroll) = setup_viewport();
        let time_zoom = 1.0;
        let keyboard_width = 120.0;
        let canvas_width = 800.0;
        {
            let mut vp = Viewport::new(&mut view, &mut max_scroll);
            vp.set_scroll_x(-100.0, keyboard_width, canvas_width, time_zoom);
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

    /// 缩放以鼠标为中心：X 轴缩放前后，鼠标指针下的 tick 保持不动
    /// （对应 yinhe 的 `zoom_around_x_preserves_tick` 不变式）
    #[test]
    fn test_set_zoom_x_keeps_tick_under_pointer() {
        let (mut view, mut max_scroll) = setup_viewport();
        view.scroll_x = 200.0;
        view.zoom_x = 2.0;
        let kbw = 120.0;
        let canvas_w = 800.0;
        let viewport_w = canvas_w - kbw;

        // 指针位于内容区 40% 处，记下缩放前的 tick
        let pointer_x = kbw + viewport_w * 0.4;
        let zoom_before = view.zoom_x;
        let tick_before = view.x_to_tick(pointer_x);
        // 注意：x_to_tick 依赖 x 为画布局部坐标，这里 pointer_x 即局部坐标（无 bounds 偏移）

        {
            let total_ticks = view.total_ticks;
            let mut vp = Viewport::new(&mut view, &mut max_scroll);
            vp.update_max_scroll(total_ticks);
            vp.set_zoom_x(zoom_before * 1.5, 0.4, kbw, canvas_w, 0.001, 10.0);
        }
        let tick_after = view.x_to_tick(pointer_x);
        assert!(
            (tick_before - tick_after).abs() < 1e-3,
            "X 轴缩放后指针下的 tick 漂移: before={tick_before}, after={tick_after}"
        );
    }

    /// 缩放以鼠标为中心：Y 轴缩放前后，鼠标指针下的 key 保持不动
    #[test]
    fn test_set_zoom_y_keeps_key_under_pointer() {
        let (mut view, mut max_scroll) = setup_viewport();
        // 选择远离边界的状态：内容高度远超视口（128 键 × 30 zoom），
        // 避免缩放后的 scroll clamp 到 [0, max] 端点干扰锚点断言
        view.scroll_y = 1000.0;
        view.zoom_y = 30.0;
        let canvas_h = 600.0;
        let ruler_h = view.ruler_height;
        let viewport_h = canvas_h - ruler_h;

        // 指针位于内容区 60% 处，记下缩放前的 key
        let pointer_y = ruler_h + viewport_h * 0.6;
        let zoom_before = view.zoom_y;
        let key_before = view.y_to_key(pointer_y);

        {
            let total_ticks = view.total_ticks;
            let mut vp = Viewport::new(&mut view, &mut max_scroll);
            vp.update_max_scroll(total_ticks);
            vp.set_zoom_y(zoom_before * 0.7, 0.6, canvas_h, 0.5, 100.0);
        }
        let key_after = view.y_to_key(pointer_y);
        assert!(
            (key_before as i32 - key_after as i32).abs() <= 1,
            "Y 轴缩放后指针下的 key 漂移: before={key_before}, after={key_after}"
        );
    }
}
