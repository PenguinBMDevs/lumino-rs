//! 中立的 2D 几何类型（替代 `iced_core::Point` / `iced_core::Size`）
//!
//! 这些类型用于 `Message` / `EditorAction` 等跨模块消息。定义在 `lumino-message`
//! （domain 层）中，使 domain 层**不依赖 UI 框架**（`iced_core`）。
//!
//! UI 层在消息边界处按需与 `iced_core` 类型互转，保持调用链不变。

/// 中立的 2D 点
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    /// 横向坐标
    pub x: f32,
    /// 纵向坐标
    pub y: f32,
}

impl Point2 {
    /// 构造新的 2D 点
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// 中立的 2D 尺寸
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size2 {
    /// 宽度
    pub width: f32,
    /// 高度
    pub height: f32,
}

impl Size2 {
    /// 构造新的 2D 尺寸
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// 中立的矩形（左上角 + 宽高）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect2 {
    /// 左缘。
    pub x: f32,
    /// 上缘。
    pub y: f32,
    /// 宽度。
    pub width: f32,
    /// 高度。
    pub height: f32,
}

impl Rect2 {
    /// 构造矩形。
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 是否包含某点（含边界）。
    pub fn contains(&self, p: Point2) -> bool {
        p.x >= self.x && p.x <= self.x + self.width && p.y >= self.y && p.y <= self.y + self.height
    }
}

impl std::ops::Sub for Point2 {
    type Output = Point2;

    fn sub(self, rhs: Self) -> Self::Output {
        Point2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Add for Point2 {
    type Output = Point2;

    fn add(self, rhs: Self) -> Self::Output {
        Point2::new(self.x + rhs.x, self.y + rhs.y)
    }
}
