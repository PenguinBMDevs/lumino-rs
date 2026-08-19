use super::CacheInvalidation;
use lumino_core::storage::config::{EraserBehavior, SelectionBoxMode};
use lumino_editor_state::editor_state::viewport::Viewport;
use lumino_ui_core::constants::editor::{MAX_VISIBLE_KEY_COUNT, MIN_VISIBLE_KEY_COUNT};

impl super::Editor {
    // 键盘设置 — 视口相关操作通过 viewport 模块，其余直接修改 ViewState

    /// 设置可见键数（向上扩展键时同步调整滚动偏移以保持视野稳定）。
    ///
    /// # 参数
    /// * `count` — 目标可见键数
    pub fn set_visible_key_count(&mut self, count: u16) {
        let old_count = self.editor_state.view.visible_key_count;
        let canvas_height = self.editor_state.canvas.size_y;

        Viewport::new(
            &mut self.editor_state.view,
            &mut self.editor_state.max_scroll,
        )
        .set_visible_key_count(
            count,
            MIN_VISIBLE_KEY_COUNT,
            MAX_VISIBLE_KEY_COUNT,
            canvas_height,
        );

        // 向上拓展：高键号在上方，扩展键（128-255）应出现在原有键位之上
        // 原有最高键(127)的 world_y 从 0 变为 added_keys*zoom_y
        // 需要同步增加 scroll_y 使原有可见区域保持不变
        if count > old_count {
            let added_keys = (count - old_count) as f32;
            self.editor_state.view.scroll_y += added_keys * self.editor_state.view.zoom_y;
            // 重新钳位到有效范围
            let vh = (canvas_height - self.editor_state.view.ruler_height).max(0.0);
            let max_sy = (self.editor_state.max_scroll.1 - vh).max(0.0);
            self.editor_state.view.scroll_y = self.editor_state.view.scroll_y.clamp(0.0, max_sy);
        }

        // 键盘和网格缓存都需要刷新：key_count 变了，键盘绘制和网格线都要重绘
        self.invalidate_caches(super::CacheInvalidation::ALL);
    }

    /// 获取当前可见键数。
    ///
    /// # 返回
    /// 可见键个数。
    pub fn visible_key_count(&self) -> u16 {
        self.editor_state.view.visible_key_count
    }

    /// 设置键盘栏宽度（像素）。
    ///
    /// # 参数
    /// * `width` — 键盘栏宽度
    pub fn set_keyboard_width(&mut self, width: f32) {
        self.editor_state.view.set_keyboard_width(width);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    /// 获取当前键盘栏宽度。
    ///
    /// # 返回
    /// 键盘栏宽度（像素）。
    pub fn keyboard_width(&self) -> f32 {
        self.editor_state.view.keyboard_width
    }

    // 音符设置

    /// 设置吸附精度（tick）。
    ///
    /// # 参数
    /// * `precision` — 吸附精度值
    pub fn set_snap_precision(&mut self, precision: f32) {
        self.editor_state.view.set_snap_precision(precision);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    /// 获取当前吸附精度。
    ///
    /// # 返回
    /// 吸附精度值（tick）。
    pub fn snap_precision(&self) -> f32 {
        self.editor_state.view.snap_precision
    }

    /// 设置新建音符的默认时值。
    ///
    /// # 参数
    /// * `length` — 默认音符长度（tick）
    pub fn set_default_note_length(&mut self, length: f32) {
        self.editor_state.view.set_default_note_length(length);
        self.invalidate_caches(CacheInvalidation::GRID);
    }

    /// 获取新建音符的默认时值。
    ///
    /// # 返回
    /// 默认音符长度（tick）。
    pub fn default_note_length(&self) -> f32 {
        self.editor_state.view.default_note_length
    }

    /// 设置橡皮擦工具行为。
    ///
    /// # 参数
    /// * `behavior` — 橡皮擦行为模式
    pub fn set_eraser_behavior(&mut self, behavior: EraserBehavior) {
        self.editor_state.view.set_eraser_behavior(behavior);
    }

    /// 获取当前橡皮擦工具行为。
    ///
    /// # 返回
    /// 橡皮擦行为模式。
    pub fn eraser_behavior(&self) -> EraserBehavior {
        self.editor_state.view.eraser_behavior
    }

    /// 设置框选框模式。
    ///
    /// # 参数
    /// * `mode` — 框选框模式
    pub fn set_selection_box_mode(&mut self, mode: SelectionBoxMode) {
        self.editor_state.view.set_selection_box_mode(mode);
    }

    /// 获取当前框选框模式。
    ///
    /// # 返回
    /// 框选框模式。
    pub fn selection_box_mode(&self) -> SelectionBoxMode {
        self.editor_state.view.selection_box_mode
    }
}
