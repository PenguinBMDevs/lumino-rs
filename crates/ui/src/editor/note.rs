use iced_core::{Color, Point, Theme};
use lumino_gfx::NoteInstance;

use crate::editor::state::ViewState;

/// 音符逻辑表示 - 包含屏幕坐标和样式信息
pub struct Note {
    /// 左上角位置 (x, y)
    pub position: Point,
    /// 尺寸 (width, height)
    pub size: Point,
    /// 颜色 (RGBA)
    pub color: Color,
}

impl Note {
    /// 创建新的音符
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: Color) -> Self {
        Self {
            position: Point::new(x, y),
            size: Point::new(width, height),
            color,
        }
    }

    /// 从鼠标位置和视图状态创建音符
    pub fn from_mouse_position(mouse_pos: Point, view_state: &ViewState, theme: &Theme) -> Self {
        // 从主题获取音符颜色
        let color = theme.extended_palette().background.strong.color;

        // 将鼠标坐标转换为 tick 坐标
        // screen_x = tick * zoom - scroll + keyboard_width
        // tick = (screen_x - keyboard_width + scroll) / zoom
        let adjusted_x = mouse_pos.x - view_state.keyboard_width + view_state.scroll_x;
        let tick_x = adjusted_x / view_state.zoom_x;
        
        // 按精度对齐 tick 坐标
        let snapped_tick_x = (tick_x / view_state.snap_precision).round() * view_state.snap_precision;

        // 计算像素对齐的屏幕坐标
        let snapped_x = (snapped_tick_x * view_state.zoom_x 
            - view_state.scroll_x 
            + view_state.keyboard_width
        ).round();

        // Y 坐标向上贴合并对齐到像素
        let snapped_y = ((mouse_pos.y / view_state.zoom_y).floor() * view_state.zoom_y).round();

        // 音符尺寸对齐到像素
        let note_width = (view_state.default_note_length * view_state.zoom_x).round();
        let note_height = view_state.zoom_y.round();

        Self::new(snapped_x, snapped_y, note_width, note_height, color)
    }

    /// 转换为 wgpu 渲染用的实例数据
    pub fn to_instance(&self) -> NoteInstance {
        NoteInstance::new(
            self.position.x,
            self.position.y,
            self.size.x,
            self.size.y,
            color_to_array(self.color),
        )
    }
}

/// 将 iced Color 转换为 [f32; 4] RGBA
fn color_to_array(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_creation() {
        let note = Note::new(100.0, 200.0, 50.0, 20.0, Color::WHITE);
        assert_eq!(note.position.x, 100.0);
        assert_eq!(note.size.x, 50.0);
    }

    #[test]
    fn test_to_instance() {
        let note = Note::new(10.0, 20.0, 30.0, 40.0, Color::from_rgba(1.0, 0.5, 0.0, 1.0));
        let instance = note.to_instance();
        
        assert_eq!(instance.position, [10.0, 20.0]);
        assert_eq!(instance.size, [30.0, 40.0]);
        assert_eq!(instance.color, [1.0, 0.5, 0.0, 1.0]);
    }
}
