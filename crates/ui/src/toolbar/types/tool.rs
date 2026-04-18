//! 工具类型和工具栏常量

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Pointer,
    Pencil,
    Brush,
    Pen,
    Eraser,
    Razor,
}

pub const DEFAULT_HEIGHT: f32 = 72.0;
pub const MIN_HEIGHT: f32 = 56.0;
pub const MAX_HEIGHT: f32 = 200.0;
pub const RESIZE_HANDLE_HEIGHT: f32 = 6.0;
