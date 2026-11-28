//! 主题色板（移植自 旧版 0.14，颜色类型换成本地 geom::Color）。
//!
//! 结构：`themes`（22 套内置常量）、`generate`（扩展色推导）、`color`（OKLCH 运算）。
//! 对外路径不变，`Palette::TOKYO_NIGHT_STORM` 等常量仍从本模块根访问。

mod color;
mod generate;
mod themes;

use crate::geom::Color;

pub use color::{darken, deviate, is_dark, lighten, mix, readable};
pub use generate::{
    Background, Danger, Extended, Pair, Primary, Secondary, Success, Tone, Warning,
};

/// A color palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// The background [`Color`] of the [`Palette`].
    pub background: Color,
    /// The text [`Color`] of the [`Palette`].
    pub text: Color,
    /// The primary [`Color`] of the [`Palette`].
    pub primary: Color,
    /// The success [`Color`] of the [`Palette`].
    pub success: Color,
    /// The warning [`Color`] of the [`Palette`].
    pub warning: Color,
    /// The danger [`Color`] of the [`Palette`].
    pub danger: Color,
}
