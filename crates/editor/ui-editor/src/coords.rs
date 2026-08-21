//! 坐标转换 — 委托到 ViewState，纵向卷帘转置支持
use iced_core::Point;

impl super::Editor {
    pub(super) fn tick_to_x(&self, tick: f32) -> f32 {
        self.editor_state.view.tick_to_x(tick)
    }
    pub(super) fn key_to_y(&self, key: u16) -> f32 {
        self.editor_state.view.key_to_y(key)
    }
    pub(super) fn x_to_tick(&self, x: f32) -> f32 {
        self.editor_state.view.x_to_tick(x)
    }
    pub(super) fn y_to_key(&self, y: f32) -> u16 {
        self.editor_state.view.y_to_key(y)
    }
    pub(super) fn snap_tick(&self, tick: f32) -> f32 {
        self.editor_state.view.snap_tick(tick)
    }

    // ── 纵向卷帘坐标转换（转置：tick ↔ Y，key ↔ X）──

    /// 逻辑 tick → 屏幕坐标（纵向返回 Y，横向返回 X 的封装）
    /// 注意：仅在需要统一 pos 转换时使用，旧 tick_to_x/key_to_y 保持横向语义不变。
    pub(super) fn pos_to_tick(&self, pos: Point) -> f32 {
        if self.editor_state.is_vertical_roll {
            let view = &self.editor_state.view;
            let canvas_h = self.editor_state.canvas.size_y;
            let kb_h = view.keyboard_width;
            let grid_bottom = canvas_h - kb_h;
            // tick = (grid_bottom - screen_y + scroll_x)/zoom_x
            (grid_bottom - pos.y + view.scroll_x) / view.zoom_x
        } else {
            self.x_to_tick(pos.x)
        }
    }

    /// 逻辑 key → 屏幕坐标对应 key（纵向从 X，横向从 Y）
    pub(super) fn pos_to_key(&self, pos: Point) -> u16 {
        if self.editor_state.is_vertical_roll {
            let view = &self.editor_state.view;
            let max = view.visible_key_count.saturating_sub(1) as f32;
            let key_f = (pos.x + view.scroll_y) / view.zoom_y;
            key_f.round().clamp(0.0, max) as u16
        } else {
            self.y_to_key(pos.y)
        }
    }

    /// 逻辑 key 原始浮点值（中间锚点自由定位用，纵向从 X）
    pub(super) fn pos_to_raw_key(&self, pos: Point) -> f32 {
        if self.editor_state.is_vertical_roll {
            let view = &self.editor_state.view;
            (pos.x + view.scroll_y) / view.zoom_y
        } else {
            let view = &self.editor_state.view;
            let max = (view.visible_key_count - 1) as f32;
            max - (pos.y - view.ruler_height + view.scroll_y) / view.zoom_y
        }
    }

    /// 逻辑 (tick,key) → 屏幕局部坐标
    pub(super) fn tick_key_to_pos(&self, tick: f32, key: u16) -> Point {
        self.tick_key_to_pos_f32(tick, key as f32)
    }

    /// 逻辑 (tick,key_f32) → 屏幕局部坐标（key 支持浮点）
    pub(super) fn tick_key_to_pos_f32(&self, tick: f32, key_f: f32) -> Point {
        if self.editor_state.is_vertical_roll {
            let view = &self.editor_state.view;
            let canvas_h = self.editor_state.canvas.size_y;
            let kb_h = view.keyboard_width;
            let grid_bottom = canvas_h - kb_h;
            let x = key_f * view.zoom_y - view.scroll_y;
            let y = grid_bottom - tick * view.zoom_x + view.scroll_x;
            Point::new(x, y)
        } else {
            let x = self.tick_to_x(tick);
            // key_f → y：key_to_y 取整，先算浮点再线性插值
            let view = &self.editor_state.view;
            let max = (view.visible_key_count - 1) as f32;
            let y = (max - key_f) * view.zoom_y - view.scroll_y + view.ruler_height;
            Point::new(x, y)
        }
    }

    /// tick → 屏幕 Y（仅纵向）
    #[allow(dead_code)]
    pub(super) fn tick_to_y_vertical(&self, tick: f32) -> f32 {
        let view = &self.editor_state.view;
        let canvas_h = self.editor_state.canvas.size_y;
        let kb_h = view.keyboard_width;
        let grid_bottom = canvas_h - kb_h;
        grid_bottom - tick * view.zoom_x + view.scroll_x
    }

    /// key → 屏幕 X（仅纵向）
    #[allow(dead_code)]
    pub(super) fn key_to_x_vertical(&self, key: u16) -> f32 {
        let view = &self.editor_state.view;
        key as f32 * view.zoom_y - view.scroll_y
    }

    /// 屏幕 Y → tick（仅纵向）
    #[allow(dead_code)]
    pub(super) fn y_to_tick_vertical(&self, y: f32) -> f32 {
        let view = &self.editor_state.view;
        let canvas_h = self.editor_state.canvas.size_y;
        let kb_h = view.keyboard_width;
        let grid_bottom = canvas_h - kb_h;
        (grid_bottom - y + view.scroll_x) / view.zoom_x
    }

    /// 屏幕 X → key（仅纵向）
    #[allow(dead_code)]
    pub(super) fn x_to_key_vertical(&self, x: f32) -> u16 {
        let view = &self.editor_state.view;
        let max = view.visible_key_count.saturating_sub(1) as f32;
        let key_f = (x + view.scroll_y) / view.zoom_y;
        key_f.round().clamp(0.0, max) as u16
    }
}
