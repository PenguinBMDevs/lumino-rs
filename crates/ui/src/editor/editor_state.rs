//! 编辑器状态管理模块
//!
//! 将 Editor 的视图状态、交互状态、工具状态等集中管理

use crate::toolbar::Tool;
use iced_core::Point;
use lumino_core::storage::config::{AutoScrollConfig, EraserBehavior};

pub mod canvas;
pub mod data;
pub mod interaction;
pub mod view;

pub use canvas::CanvasState;
pub use data::EditorData;
pub use interaction::{EditState, HitType, InteractionState, SelectionHitType};
pub use view::ViewState;

/// 编辑器完整状态
#[derive(Debug)]
pub struct EditorState {
    /// 视图状态（滚动、缩放）
    pub view: ViewState,
    /// Canvas 状态（尺寸、偏移）
    pub canvas: CanvasState,
    /// 交互状态（编辑状态、悬停、选中）
    pub interaction: InteractionState,
    /// 当前工具
    pub tool: Tool,
    /// 自动滚动配置
    pub auto_scroll: AutoScrollConfig,
    /// 最大滚动范围
    pub max_scroll: Point,
    /// 音符数据（音轨管理、文档引用）
    pub data: EditorData,
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorState {
    pub fn new() -> Self {
        let view = ViewState::default();
        Self {
            max_scroll: Point::new(
                view.total_ticks as f32 * view.zoom_x,
                view.visible_key_count as f32 * view.zoom_y,
            ),
            view,
            canvas: CanvasState::default(),
            interaction: InteractionState::default(),
            data: EditorData::new(),
            tool: Tool::Pointer,
            auto_scroll: AutoScrollConfig::default(),
        }
    }

    /// 更新最大滚动范围
    pub fn update_max_scroll(&mut self, total_ticks: u32) {
        self.max_scroll = Point::new(
            total_ticks as f32 * self.view.zoom_x,
            self.view.visible_key_count as f32 * self.view.zoom_y,
        );
    }

    /// 设置当前工具
    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        // 切换工具时清除选中状态
        if tool != Tool::Pointer {
            self.interaction.selected_notes.clear();
        }
    }

    /// 获取当前工具
    pub fn current_tool(&self) -> Tool {
        self.tool
    }

    /// 更新鼠标位置
    pub fn update_cursor_position(&mut self, position: Option<Point>) {
        self.canvas.cursor_position = position;
    }

    /// 更新 Canvas 偏移量
    pub fn set_canvas_offset(&mut self, offset: Point) {
        self.canvas.offset = offset;
    }

    /// 更新 Canvas 尺寸
    pub fn set_canvas_size(&mut self, size: Point) {
        self.canvas.size = size;
    }

    // ── 视图/滚动方法 ──

    /// 获取视图状态引用
    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// 获取视图状态可变引用
    pub fn view_mut(&mut self) -> &mut ViewState {
        &mut self.view
    }

    /// 设置横向滚动（带有有效范围限制）
    /// 直接设置位置，同步平滑滚动目标并停止动画
    pub fn set_scroll_x(&mut self, scroll_x: f32, keyboard_width: f32, canvas_width: f32) {
        let total_width = self.view.total_ticks as f32 * self.view.zoom_x;
        let viewport_width = (canvas_width - keyboard_width).max(0.0);
        let effective_max_scroll = (total_width - viewport_width).max(0.0);
        let clamped = scroll_x.max(0.0).min(effective_max_scroll);
        self.view.scroll_x = clamped;
        // 直接设置时同步目标位置，避免动画冲突
        self.view.smooth_scroll.target_x = clamped;
        self.view.smooth_scroll.active = false;
    }

    /// 设置纵向滚动（带有有效范围限制）
    /// 直接设置位置，同步平滑滚动目标并停止动画
    pub fn set_scroll_y(&mut self, scroll_y: f32, canvas_height: f32) {
        let total_height = self.view.visible_key_count as f32 * self.view.zoom_y;
        let viewport_height = (canvas_height - self.view.ruler_height).max(0.0);
        let effective_max_scroll = (total_height - viewport_height).max(0.0);
        let clamped = scroll_y.max(0.0).min(effective_max_scroll);
        self.view.scroll_y = clamped;
        // 直接设置时同步目标位置，避免动画冲突
        self.view.smooth_scroll.target_y = clamped;
        self.view.smooth_scroll.active = false;
    }

    /// 平滑滚动偏移（用于鼠标滚轮）
    /// 设置目标位置并启动动画，不直接修改当前位置
    pub fn smooth_scroll_by(&mut self, delta_x: f32, delta_y: f32) {
        let canvas = &self.canvas;
        let v = &mut self.view;

        // 计算新的目标位置
        let new_target_x = v.smooth_scroll.target_x - delta_x;
        let new_target_y = v.smooth_scroll.target_y - delta_y;

        // X 轴范围限制
        let total_width = v.total_ticks as f32 * v.zoom_x;
        let viewport_width = (canvas.size.x - v.keyboard_width).max(0.0);
        let effective_max_scroll_x = (total_width - viewport_width).max(0.0);
        let clamped_target_x = new_target_x.max(0.0).min(effective_max_scroll_x);

        // Y 轴范围限制
        let total_height = v.visible_key_count as f32 * v.zoom_y;
        let viewport_height = (canvas.size.y - v.ruler_height).max(0.0);
        let effective_max_scroll_y = (total_height - viewport_height).max(0.0);
        let clamped_target_y = new_target_y.max(0.0).min(effective_max_scroll_y);

        v.smooth_scroll
            .set_target(clamped_target_x, clamped_target_y);
    }

    /// 更新平滑滚动动画
    /// 返回是否仍在动画中
    pub fn update_smooth_scroll(&mut self) -> bool {
        let v = &mut self.view;
        if !v.smooth_scroll.active {
            return false;
        }

        let (new_x, new_y, still_active) = v.smooth_scroll.update(v.scroll_x, v.scroll_y);

        v.scroll_x = new_x;
        v.scroll_y = new_y;
        v.smooth_scroll.active = still_active;

        still_active
    }

    /// 设置横向缩放
    pub fn set_zoom_x(
        &mut self,
        zoom_x: f32,
        fixed_ratio: f32,
        keyboard_width: f32,
        canvas_width: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) {
        let old_zoom_x = self.view.zoom_x;
        self.view.zoom_x = zoom_x.clamp(min_zoom, max_zoom);

        let ratio = self.view.zoom_x / old_zoom_x;
        let view_width = (canvas_width - keyboard_width).max(0.0);

        // 保持固定比例处的 tick 不变
        let fixed_pixel = self.view.scroll_x + view_width * fixed_ratio;
        self.view.scroll_x = fixed_pixel * ratio - view_width * fixed_ratio;

        self.update_max_scroll(self.view.total_ticks);

        // 使用有效最大滚动值
        let viewport_width = (canvas_width - keyboard_width).max(0.0);
        let effective_max_scroll = (self.max_scroll.x - viewport_width).max(0.0);
        self.view.scroll_x = self.view.scroll_x.max(0.0).min(effective_max_scroll);
    }

    /// 设置纵向缩放
    pub fn set_zoom_y(
        &mut self,
        zoom_y: f32,
        fixed_ratio: f32,
        canvas_height: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) {
        let old_zoom_y = self.view.zoom_y;
        self.view.zoom_y = zoom_y.clamp(min_zoom, max_zoom);

        let ratio = self.view.zoom_y / old_zoom_y;
        let view_height = canvas_height.max(0.0);

        // 保持固定比例处的 key 不变
        let fixed_pixel = self.view.scroll_y + view_height * fixed_ratio;
        self.view.scroll_y = fixed_pixel * ratio - view_height * fixed_ratio;

        self.update_max_scroll(self.view.total_ticks);

        // 使用有效最大滚动值
        let viewport_height = (canvas_height - self.view.ruler_height).max(0.0);
        let effective_max_scroll = (self.max_scroll.y - viewport_height).max(0.0);
        self.view.scroll_y = self.view.scroll_y.max(0.0).min(effective_max_scroll);
    }

    /// 设置可见琴键数量
    pub fn set_visible_key_count(
        &mut self,
        count: u16,
        min_count: u16,
        max_count: u16,
        canvas_height: f32,
    ) {
        let clamped_count = count.clamp(min_count, max_count);
        self.view.visible_key_count = clamped_count;
        self.update_max_scroll(self.view.total_ticks);

        // 使用有效最大滚动值
        let viewport_height = (canvas_height - self.view.ruler_height).max(0.0);
        let effective_max_scroll = (self.max_scroll.y - viewport_height).max(0.0);
        if self.view.scroll_y > effective_max_scroll {
            self.view.scroll_y = effective_max_scroll;
        }
    }

    /// 设置键盘宽度
    pub fn set_keyboard_width(&mut self, width: f32) {
        self.view.keyboard_width = width.max(0.0);
    }

    /// 设置对齐精度
    pub fn set_snap_precision(&mut self, precision: f32) {
        self.view.snap_precision = precision.max(1.0);
    }

    /// 设置默认音符长度
    pub fn set_default_note_length(&mut self, length: f32) {
        self.view.default_note_length = length.max(1.0);
    }

    /// 设置橡皮擦行为
    pub fn set_eraser_behavior(&mut self, behavior: EraserBehavior) {
        self.view.eraser_behavior = behavior;
    }
}
