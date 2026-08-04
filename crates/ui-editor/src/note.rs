//! 音符逻辑表示 — 重新导出自 lumino-core
//!
//! 保持与原有 `crate::note::*` 路径完全兼容。
//! UI 特有的方法（screen_bounds, to_instance）通过扩展 trait 定义在此处。

use iced_core::{Color, Point, Rectangle, Size};
use lumino_gfx::NoteInstance;

pub use lumino_note_core::Note;

use crate::editor_state::ViewState;

/// UI 特有的 Note 扩展方法
pub trait NoteExt {
    /// 计算音符在屏幕上的边界矩形
    fn screen_bounds(&self, view_state: &ViewState) -> Rectangle;
    /// 转换为 GPU 实例
    fn to_instance(&self, color: Color, border_width: u32) -> NoteInstance;
}

impl NoteExt for Note {
    /// 计算音符在屏幕上的边界矩形
    fn screen_bounds(&self, view_state: &ViewState) -> Rectangle {
        let note_x =
            self.tick * view_state.zoom_x - view_state.scroll_x + view_state.keyboard_width;
        let max_key_index = (view_state.visible_key_count - 1) as f32;
        let note_y = (max_key_index - self.key as f32) * view_state.zoom_y - view_state.scroll_y
            + view_state.ruler_height;
        let width = self.length * view_state.zoom_x;
        let height = view_state.zoom_y;
        Rectangle::new(Point::new(note_x, note_y), Size::new(width, height))
    }

    /// 转换为 GPU 实例
    fn to_instance(&self, color: Color, border_width: u32) -> NoteInstance {
        NoteInstance::new(
            self.tick,
            self.key as f32,
            self.length,
            color_to_array(color),
            border_width,
        )
    }
}

/// 将 iced Color 转换为 [f32; 4] RGBA
pub fn color_to_array(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}
