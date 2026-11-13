//! 本地几何与颜色类型。
//!
//! 只保留本项目实际使用的子集：`Point` / `Size` / `Rectangle` / `Color` 与
//! 光标交互类型 `mouse::{Cursor, Interaction, ScrollDelta}`。字段名与语义与
//! 旧版 0.14 完全一致，保证此前按 旧版 口径写的坐标换算无需改动。

use std::ops::{Add, Sub};

/// 二维点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Point<T = f32> {
    /// X 坐标。
    pub x: T,
    /// Y 坐标。
    pub y: T,
}

impl<T> Point<T> {
    /// 由坐标创建。
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl Point {
    /// 原点 `(0, 0)`。
    pub const ORIGIN: Self = Self::new(0.0, 0.0);
}

impl Point<f32> {
    /// 到另一点的欧氏距离。
    pub fn distance(&self, to: Self) -> f32 {
        let dx = self.x - to.x;
        let dy = self.y - to.y;
        (dx * dx + dy * dy).sqrt()
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Point {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

/// 二维尺寸。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Size<T = f32> {
    /// 宽度。
    pub width: T,
    /// 高度。
    pub height: T,
}

impl<T> Size<T> {
    /// 由宽高创建。
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

impl Size<f32> {
    /// 零尺寸。
    pub const ZERO: Self = Self::new(0.0, 0.0);

    /// 逐分量取小。
    pub fn min(self, other: Self) -> Self {
        Self::new(self.width.min(other.width), self.height.min(other.height))
    }

    /// 逐分量取大。
    pub fn max(self, other: Self) -> Self {
        Self::new(self.width.max(other.width), self.height.max(other.height))
    }
}

/// 轴对齐矩形。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rectangle<T = f32> {
    /// 左上角 X。
    pub x: T,
    /// 左上角 Y。
    pub y: T,
    /// 宽。
    pub width: T,
    /// 高。
    pub height: T,
}

impl Rectangle<f32> {
    /// 由左上角与尺寸创建。
    pub const fn new(top_left: Point, size: Size) -> Self {
        Self {
            x: top_left.x,
            y: top_left.y,
            width: size.width,
            height: size.height,
        }
    }

    /// 由左上角与右下角创建。
    pub fn from_min_max(min: Point, max: Point) -> Self {
        Self {
            x: min.x,
            y: min.y,
            width: (max.x - min.x).max(0.0),
            height: (max.y - min.y).max(0.0),
        }
    }

    /// 中心点。
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// 左上角位置。
    pub fn position(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// 尺寸。
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// 面积。
    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    /// 是否包含某点（左闭右开，与 旧版 一致）。
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width
            && point.y >= self.y
            && point.y < self.y + self.height
    }

    /// 与另一矩形的交集（无交集返回 `None`）。
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        if right <= x || bottom <= y {
            return None;
        }
        Some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    /// 是否与另一矩形相交。
    pub fn intersects(&self, other: &Self) -> bool {
        self.intersection(other).is_some()
    }
}

/// 线性 RGBA 颜色（分量 0.0..=1.0）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Color {
    /// 红。
    pub r: f32,
    /// 绿。
    pub g: f32,
    /// 蓝。
    pub b: f32,
    /// 透明度。
    pub a: f32,
}

impl Color {
    /// 纯黑。
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    /// 纯白。
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    /// 全透明。
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// 由 RGB 分量创建（不透明度 1.0）。
    pub const fn from_rgb(r: f32, g: f32, b: f32) -> Color {
        Color::from_rgba(r, g, b, 1.0)
    }

    /// 由 RGBA 分量创建。
    pub const fn from_rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }

    /// 由 8 位 RGB 创建（不透明度 1.0）。
    pub const fn from_rgb8(r: u8, g: u8, b: u8) -> Color {
        Color::from_rgba8(r, g, b, 1.0)
    }

    /// 由 8 位 RGB + 浮点不透明度创建。
    pub const fn from_rgba8(r: u8, g: u8, b: u8, a: f32) -> Color {
        Color::from_rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
    }

    /// 转成 8 位 RGBA（四舍五入）。
    pub fn into_rgba8(self) -> [u8; 4] {
        [
            (self.r * 255.0).round() as u8,
            (self.g * 255.0).round() as u8,
            (self.b * 255.0).round() as u8,
            (self.a * 255.0).round() as u8,
        ]
    }

    /// 由线性 RGBA 分量创建（sRGB 编码）。
    pub fn from_linear_rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        fn gamma_component(u: f32) -> f32 {
            if u < 0.0031308 {
                12.92 * u
            } else {
                1.055 * u.powf(1.0 / 2.4) - 0.055
            }
        }

        Color::from_rgba(
            gamma_component(r),
            gamma_component(g),
            gamma_component(b),
            a,
        )
    }

    /// 转成线性 RGBA。
    pub fn into_linear(self) -> [f32; 4] {
        fn linear_component(u: f32) -> f32 {
            if u < 0.04045 {
                u / 12.92
            } else {
                ((u + 0.055) / 1.055).powf(2.4)
            }
        }

        [
            linear_component(self.r),
            linear_component(self.g),
            linear_component(self.b),
            self.a,
        ]
    }

    /// 相对亮度（WCAG 2.1）。
    pub fn relative_luminance(self) -> f32 {
        let linear = self.into_linear();
        0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    }

    /// 与另一颜色的相对对比度。
    pub fn relative_contrast(self, other: Color) -> f32 {
        let lum_a = self.relative_luminance();
        let lum_b = other.relative_luminance();

        (lum_a.max(lum_b) + 0.05) / (lum_a.min(lum_b) + 0.05)
    }

    /// 当前颜色叠在给定背景上是否可读（对比度 ≥ 6.0）。
    pub fn is_readable_on(self, background: Color) -> bool {
        background.relative_contrast(self) >= 6.0
    }
}

/// 十六进制颜色字面量（`0xRRGGBB` 或 `0xRRGGBBAA` 风格，可带 3 位缩写）。
#[macro_export]
macro_rules! color {
    ($r:expr, $g:expr, $b:expr) => {
        $crate::geom::Color::from_rgb8($r, $g, $b)
    };
    ($r:expr, $g:expr, $b:expr, $a:expr) => {
        $crate::geom::Color::from_rgba8($r, $g, $b, $a)
    };
    ($hex:literal) => {{ $crate::color!($hex, 1.0) }};
    ($hex:literal, $a:expr) => {{
        let mut hex = $hex as u32;

        // 3 位缩写：0x123 → 0x112233
        if stringify!($hex).len() == 5 {
            let r = hex & 0xF00;
            let g = hex & 0xF0;
            let b = hex & 0xF;
            hex = (r << 12) | (r << 8) | (g << 8) | (g << 4) | (b << 4) | b;
        }

        debug_assert!(hex <= 0xffffff, "color! 值不得超过 0xffffff");

        let r = ((hex & 0xff0000) >> 16) as u8;
        let g = ((hex & 0x00ff00) >> 8) as u8;
        let b = (hex & 0x0000ff) as u8;

        $crate::geom::Color::from_rgba8(r, g, b, $a)
    }};
}

/// 光标与鼠标输入类型。
pub mod mouse {
    use super::{Point, Rectangle};

    /// 光标语义（决定窗口光标图标）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum Interaction {
        /// 无特定语义。
        #[default]
        None,
        /// 隐藏光标。
        Hidden,
        /// 空闲。
        Idle,
        /// 上下文菜单。
        ContextMenu,
        /// 帮助。
        Help,
        /// 可点击。
        Pointer,
        /// 进行中。
        Progress,
        /// 等待。
        Wait,
        /// 单元格。
        Cell,
        /// 十字准星。
        Crosshair,
        /// 文本输入。
        Text,
        /// 别名 / 快捷方式。
        Alias,
        /// 复制。
        Copy,
        /// 移动。
        Move,
        /// 禁止放置。
        NoDrop,
        /// 禁止操作。
        NotAllowed,
        /// 可抓取。
        Grab,
        /// 抓取中。
        Grabbing,
        /// 水平缩放。
        ResizingHorizontally,
        /// 垂直缩放。
        ResizingVertically,
        /// 左上↔右下缩放。
        ResizingDiagonallyUp,
        /// 右上↔左下缩放。
        ResizingDiagonallyDown,
        /// 列宽缩放。
        ResizingColumn,
        /// 行高缩放。
        ResizingRow,
        /// 全向滚动。
        AllScroll,
        /// 放大。
        ZoomIn,
        /// 缩小。
        ZoomOut,
    }

    /// 光标位置（不可用 / 可用）。
    #[derive(Debug, Clone, Copy, PartialEq, Default)]
    pub enum Cursor {
        /// 指针不在窗口内。
        #[default]
        Unavailable,
        /// 指针位于给定位置。
        Available(Point),
    }

    impl Cursor {
        /// 取当前位置。
        pub fn position(self) -> Option<Point> {
            match self {
                Cursor::Unavailable => None,
                Cursor::Available(position) => Some(position),
            }
        }

        /// 是否位于给定矩形内。
        pub fn is_over(self, bounds: Rectangle) -> bool {
            self.position()
                .is_some_and(|position| bounds.contains(position))
        }
    }

    /// 滚轮增量。
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum ScrollDelta {
        /// 按刻度线计。
        Lines {
            /// 水平增量。
            x: f32,
            /// 垂直增量。
            y: f32,
        },
        /// 按像素计。
        Pixels {
            /// 水平增量。
            x: f32,
            /// 垂直增量。
            y: f32,
        },
    }

    /// 鼠标按键。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Button {
        /// 左键。
        Left,
        /// 右键。
        Right,
        /// 中键。
        Middle,
        /// 后退。
        Back,
        /// 前进。
        Forward,
        /// 其他。
        Other(u8),
    }
}
