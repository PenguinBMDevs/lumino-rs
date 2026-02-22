use iced_core::{Color, Point, Rectangle, Size};
use lumino_gfx::NoteInstance;

use crate::editor::state::ViewState;

/// 音符逻辑表示
#[derive(Debug, Clone)]
pub struct Note {
    pub tick: f32,
    pub key: u16,
    pub length: f32,
}

impl Note {
    pub fn new(tick: f32, key: u16, length: f32) -> Self {
        Self { tick, key, length }
    }

    pub fn screen_bounds(&self, view_state: &ViewState) -> Rectangle {
        let x = self.tick * view_state.zoom_x - view_state.scroll_x + view_state.keyboard_width;
        let max_key_index = (view_state.visible_key_count - 1) as f32;
        let y = (max_key_index - self.key as f32) * view_state.zoom_y - view_state.scroll_y;
        let width = self.length * view_state.zoom_x;
        let height = view_state.zoom_y;
        Rectangle::new(Point::new(x, y), Size::new(width, height))
    }

    pub fn to_instance(&self, view_state: &ViewState, color: Color) -> NoteInstance {
        let bounds = self.screen_bounds(view_state);
        NoteInstance::new(
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            color_to_array(color),
        )
    }
}

/// 将 iced Color 转换为 [f32; 4] RGBA
pub fn color_to_array(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}
