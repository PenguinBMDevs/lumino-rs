//! 由基础色推导的扩展色：背景 / 主色 / 次色 / 状态色。
use crate::geom::Color;

use super::Palette;
use super::color::{deviate, is_dark, mix, readable};

/// An extended set of colors generated from a [`Palette`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extended {
    /// The set of background colors.
    pub background: Background,
    /// The set of primary colors.
    pub primary: Primary,
    /// The set of secondary colors.
    pub secondary: Secondary,
    /// The set of success colors.
    pub success: Success,
    /// The set of warning colors.
    pub warning: Warning,
    /// The set of danger colors.
    pub danger: Danger,
    /// Whether the palette is dark or not.
    pub is_dark: bool,
}

impl Extended {
    /// Generates an [`Extended`] palette from a simple [`Palette`].
    pub fn generate(palette: Palette) -> Self {
        Self {
            background: Background::new(palette.background, palette.text),
            primary: Primary::generate(palette.primary, palette.background, palette.text),
            secondary: Secondary::generate(palette.background, palette.text),
            success: Success::generate(palette.success, palette.background, palette.text),
            warning: Warning::generate(palette.warning, palette.background, palette.text),
            danger: Danger::generate(palette.danger, palette.background, palette.text),
            is_dark: is_dark(palette.background),
        }
    }
}

/// A pair of background and text colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    /// The background color.
    pub color: Color,

    /// The text color.
    ///
    /// It's guaranteed to be readable on top of the background [`color`].
    ///
    /// [`color`]: Self::color
    pub text: Color,
}

impl Pair {
    /// Creates a new [`Pair`] from a background [`Color`] and some text [`Color`].
    pub fn new(color: Color, text: Color) -> Self {
        Self {
            color,
            text: readable(color, text),
        }
    }
}

/// A set of background colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Background {
    /// The base background color.
    pub base: Pair,
    /// The weakest version of the base background color.
    pub weakest: Pair,
    /// A weaker version of the base background color.
    pub weaker: Pair,
    /// A weak version of the base background color.
    pub weak: Pair,
    /// A neutral version of the base background color, between weak and strong.
    pub neutral: Pair,
    /// A strong version of the base background color.
    pub strong: Pair,
    /// A stronger version of the base background color.
    pub stronger: Pair,
    /// The strongest version of the base background color.
    pub strongest: Pair,
}

impl Background {
    /// Generates a set of [`Background`] colors from the base and text colors.
    pub fn new(base: Color, text: Color) -> Self {
        let weakest = deviate(base, 0.03);
        let weaker = deviate(base, 0.07);
        let weak = deviate(base, 0.1);
        let neutral = deviate(base, 0.125);
        let strong = deviate(base, 0.15);
        let stronger = deviate(base, 0.175);
        let strongest = deviate(base, 0.20);

        Self {
            base: Pair::new(base, text),
            weakest: Pair::new(weakest, text),
            weaker: Pair::new(weaker, text),
            weak: Pair::new(weak, text),
            neutral: Pair::new(neutral, text),
            strong: Pair::new(strong, text),
            stronger: Pair::new(stronger, text),
            strongest: Pair::new(strongest, text),
        }
    }
}

/// A set of accent colors (primary / success / warning / danger share one shape).
///
/// The four roles previously had four byte-identical `struct` + `impl` pairs; a
/// theme now only supplies the base color, and every role is an alias of this
/// type, so adding or retuning one place cannot drift from the others.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tone {
    /// The base color.
    pub base: Pair,
    /// A weaker version of the base color.
    pub weak: Pair,
    /// A stronger version of the base color.
    pub strong: Pair,
}

impl Tone {
    /// Generates a set of [`Tone`] colors from the base, background, and text colors.
    pub fn generate(base: Color, background: Color, text: Color) -> Self {
        let weak = mix(base, background, 0.4);
        let strong = deviate(base, 0.1);

        Self {
            base: Pair::new(base, text),
            weak: Pair::new(weak, text),
            strong: Pair::new(strong, text),
        }
    }
}

/// A set of primary colors.
pub type Primary = Tone;
/// A set of success colors.
pub type Success = Tone;
/// A set of warning colors.
pub type Warning = Tone;
/// A set of danger colors.
pub type Danger = Tone;

/// A set of secondary colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Secondary {
    /// The base secondary color.
    pub base: Pair,
    /// A weaker version of the base secondary color.
    pub weak: Pair,
    /// A stronger version of the base secondary color.
    pub strong: Pair,
}

impl Secondary {
    /// Generates a set of [`Secondary`] colors from the base and text colors.
    pub fn generate(base: Color, text: Color) -> Self {
        let factor = if is_dark(base) { 0.2 } else { 0.4 };

        let weak = mix(deviate(base, 0.1), text, factor);
        let strong = mix(deviate(base, 0.3), text, factor);
        let base = mix(deviate(base, 0.2), text, factor);

        Self {
            base: Pair::new(base, text),
            weak: Pair::new(weak, text),
            strong: Pair::new(strong, text),
        }
    }
}
