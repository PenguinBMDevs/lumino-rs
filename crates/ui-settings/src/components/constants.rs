//! 设置面板常量定义

// 图标尺寸
pub const ICON_SIZE_SMALL: u32 = 18;

// 文本尺寸 (使用 f32 以兼容 iced_core::Pixels)
pub const TEXT_SIZE_LABEL: f32 = 14.0;
pub const TEXT_SIZE_ARROW: f32 = 12.0;
pub const TEXT_SIZE_TITLE: f32 = 18.0;
pub const TEXT_SIZE_SECTION: f32 = 16.0;
pub const TEXT_SIZE_CONTENT: f32 = 14.0;

// 布局尺寸
pub const MENU_WIDTH: f32 = 200.0;
pub const ICON_CONTAINER_WIDTH: f32 = 24.0;

// 间距
pub const SPACING_ICON_LABEL: f32 = 8.0;
pub const SPACING_CONTENT: f32 = 10.0;
pub const SPACING_MAIN: f32 = 16.0;
pub const SPACING_MENU_CONTENT: f32 = 0.0;

// 内边距
pub const PADDING_ITEM_VERTICAL: f32 = 12.0;
pub const PADDING_ITEM_HORIZONTAL: f32 = 16.0;
pub const PADDING_MENU: f32 = 1.0;
pub const PADDING_CONTENT: f32 = 20.0;

// 圆角
pub const BORDER_RADIUS_MENU: f32 = 16.0;
pub const BORDER_RADIUS_CONTENT: f32 = 21.0;
pub const BORDER_WIDTH: f32 = 1.0;

// 阴影
pub const SHADOW_COLOR_MENU: [f32; 4] = [0.0, 0.0, 0.0, 0.15];
pub const SHADOW_OFFSET_MENU: (f32, f32) = (0.0, 4.0);
pub const SHADOW_BLUR_MENU: f32 = 8.0;

pub const SHADOW_COLOR_CONTENT: [f32; 4] = [0.0, 0.0, 0.0, 0.25];
pub const SHADOW_OFFSET_CONTENT: (f32, f32) = (0.0, 4.0);
pub const SHADOW_BLUR_CONTENT: f32 = 4.0;
