//! 设置面板常量定义

// 图标尺寸
/// 小图标尺寸（像素）
pub const ICON_SIZE_SMALL: u32 = 18;

// 文本尺寸 (使用 f32 以兼容 iced_core::Pixels)
/// 标签文本尺寸
pub const TEXT_SIZE_LABEL: f32 = 14.0;
/// 箭头文本尺寸
pub const TEXT_SIZE_ARROW: f32 = 12.0;
/// 标题文本尺寸
pub const TEXT_SIZE_TITLE: f32 = 18.0;
/// 章节标题文本尺寸
pub const TEXT_SIZE_SECTION: f32 = 16.0;
/// 正文内容文本尺寸
pub const TEXT_SIZE_CONTENT: f32 = 14.0;

// 布局尺寸
/// 左侧菜单宽度
pub const MENU_WIDTH: f32 = 200.0;
/// 图标容器宽度
pub const ICON_CONTAINER_WIDTH: f32 = 24.0;

// 间距
/// 图标与标签之间距
pub const SPACING_ICON_LABEL: f32 = 8.0;
/// 内容元素之间距
pub const SPACING_CONTENT: f32 = 10.0;
/// 主布局间距
pub const SPACING_MAIN: f32 = 16.0;
/// 菜单与内容之间距
pub const SPACING_MENU_CONTENT: f32 = 0.0;

// 内边距
/// 菜单项垂直内边距
pub const PADDING_ITEM_VERTICAL: f32 = 12.0;
/// 菜单项水平内边距
pub const PADDING_ITEM_HORIZONTAL: f32 = 16.0;
/// 菜单内边距
pub const PADDING_MENU: f32 = 1.0;
/// 内容区域内边距
pub const PADDING_CONTENT: f32 = 20.0;

// 圆角
/// 菜单圆角半径
pub const BORDER_RADIUS_MENU: f32 = 16.0;
/// 内容区域圆角半径
pub const BORDER_RADIUS_CONTENT: f32 = 21.0;
/// 边框宽度
pub const BORDER_WIDTH: f32 = 1.0;

// 阴影
/// 菜单阴影颜色（RGBA）
pub const SHADOW_COLOR_MENU: [f32; 4] = [0.0, 0.0, 0.0, 0.15];
/// 菜单阴影偏移
pub const SHADOW_OFFSET_MENU: (f32, f32) = (0.0, 4.0);
/// 菜单阴影模糊半径
pub const SHADOW_BLUR_MENU: f32 = 8.0;

/// 内容区域阴影颜色（RGBA）
pub const SHADOW_COLOR_CONTENT: [f32; 4] = [0.0, 0.0, 0.0, 0.25];
/// 内容区域阴影偏移
pub const SHADOW_OFFSET_CONTENT: (f32, f32) = (0.0, 4.0);
/// 内容区域阴影模糊半径
pub const SHADOW_BLUR_CONTENT: f32 = 4.0;
