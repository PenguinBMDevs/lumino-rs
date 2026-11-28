//! 内置主题色板常量（22 套，按名称排列）。
use crate::color;
use crate::geom::Color;

use super::Palette;

impl Palette {
    /// The built-in light variant of a [`Palette`].
    pub const LIGHT: Self = Self {
        background: Color::WHITE,
        text: Color::BLACK,
        primary: color!(0x5865F2),
        success: color!(0x12664f),
        warning: color!(0xb77e33),
        danger: color!(0xc3423f),
    };

    /// The built-in dark variant of a [`Palette`].
    pub const DARK: Self = Self {
        background: color!(0x2B2D31),
        text: Color::from_rgb(0.90, 0.90, 0.90),
        primary: color!(0x5865F2),
        success: color!(0x12664f),
        warning: color!(0xffc14e),
        danger: color!(0xc3423f),
    };

    /// The built-in [Dracula] variant of a [`Palette`].
    ///
    /// [Dracula]: https://draculatheme.com
    pub const DRACULA: Self = Self {
        background: color!(0x282A36), // BACKGROUND
        text: color!(0xf8f8f2),       // FOREGROUND
        primary: color!(0xbd93f9),    // PURPLE
        success: color!(0x50fa7b),    // GREEN
        warning: color!(0xf1fa8c),    // YELLOW
        danger: color!(0xff5555),     // RED
    };

    /// The built-in [Nord] variant of a [`Palette`].
    ///
    /// [Nord]: https://www.nordtheme.com/docs/colors-and-palettes
    pub const NORD: Self = Self {
        background: color!(0x2e3440), // nord0
        text: color!(0xeceff4),       // nord6
        primary: color!(0x8fbcbb),    // nord7
        success: color!(0xa3be8c),    // nord14
        warning: color!(0xebcb8b),    // nord13
        danger: color!(0xbf616a),     // nord11
    };

    /// The built-in [Solarized] Light variant of a [`Palette`].
    ///
    /// [Solarized]: https://ethanschoonover.com/solarized
    pub const SOLARIZED_LIGHT: Self = Self {
        background: color!(0xfdf6e3), // base3
        text: color!(0x657b83),       // base00
        primary: color!(0x2aa198),    // cyan
        success: color!(0x859900),    // green
        warning: color!(0xb58900),    // yellow
        danger: color!(0xdc322f),     // red
    };

    /// The built-in [Solarized] Dark variant of a [`Palette`].
    ///
    /// [Solarized]: https://ethanschoonover.com/solarized
    pub const SOLARIZED_DARK: Self = Self {
        background: color!(0x002b36), // base03
        text: color!(0x839496),       // base0
        primary: color!(0x2aa198),    // cyan
        success: color!(0x859900),    // green
        warning: color!(0xb58900),    // yellow
        danger: color!(0xdc322f),     // red
    };

    /// The built-in [Gruvbox] Light variant of a [`Palette`].
    ///
    /// [Gruvbox]: https://github.com/morhetz/gruvbox
    pub const GRUVBOX_LIGHT: Self = Self {
        background: color!(0xfbf1c7), // light BG_0
        text: color!(0x282828),       // light FG0_29
        primary: color!(0x458588),    // light BLUE_4
        success: color!(0x98971a),    // light GREEN_2
        warning: color!(0xd79921),    // light YELLOW_3
        danger: color!(0xcc241d),     // light RED_1
    };

    /// The built-in [Gruvbox] Dark variant of a [`Palette`].
    ///
    /// [Gruvbox]: https://github.com/morhetz/gruvbox
    pub const GRUVBOX_DARK: Self = Self {
        background: color!(0x282828), // dark BG_0
        text: color!(0xfbf1c7),       // dark FG0_29
        primary: color!(0x458588),    // dark BLUE_4
        success: color!(0x98971a),    // dark GREEN_2
        warning: color!(0xd79921),    // dark YELLOW_3
        danger: color!(0xcc241d),     // dark RED_1
    };

    /// The built-in [Catppuccin] Latte variant of a [`Palette`].
    ///
    /// [Catppuccin]: https://github.com/catppuccin/catppuccin
    pub const CATPPUCCIN_LATTE: Self = Self {
        background: color!(0xeff1f5), // Base
        text: color!(0x4c4f69),       // Text
        primary: color!(0x1e66f5),    // Blue
        success: color!(0x40a02b),    // Green
        warning: color!(0xdf8e1d),    // Yellow
        danger: color!(0xd20f39),     // Red
    };

    /// The built-in [Catppuccin] Frappé variant of a [`Palette`].
    ///
    /// [Catppuccin]: https://github.com/catppuccin/catppuccin
    pub const CATPPUCCIN_FRAPPE: Self = Self {
        background: color!(0x303446), // Base
        text: color!(0xc6d0f5),       // Text
        primary: color!(0x8caaee),    // Blue
        success: color!(0xa6d189),    // Green
        warning: color!(0xe5c890),    // Yellow
        danger: color!(0xe78284),     // Red
    };

    /// The built-in [Catppuccin] Macchiato variant of a [`Palette`].
    ///
    /// [Catppuccin]: https://github.com/catppuccin/catppuccin
    pub const CATPPUCCIN_MACCHIATO: Self = Self {
        background: color!(0x24273a), // Base
        text: color!(0xcad3f5),       // Text
        primary: color!(0x8aadf4),    // Blue
        success: color!(0xa6da95),    // Green
        warning: color!(0xeed49f),    // Yellow
        danger: color!(0xed8796),     // Red
    };

    /// The built-in [Catppuccin] Mocha variant of a [`Palette`].
    ///
    /// [Catppuccin]: https://github.com/catppuccin/catppuccin
    pub const CATPPUCCIN_MOCHA: Self = Self {
        background: color!(0x1e1e2e), // Base
        text: color!(0xcdd6f4),       // Text
        primary: color!(0x89b4fa),    // Blue
        success: color!(0xa6e3a1),    // Green
        warning: color!(0xf9e2af),    // Yellow
        danger: color!(0xf38ba8),     // Red
    };

    /// The built-in [Tokyo Night] variant of a [`Palette`].
    ///
    /// [Tokyo Night]: https://github.com/enkia/tokyo-night-vscode-theme
    pub const TOKYO_NIGHT: Self = Self {
        background: color!(0x1a1b26), // Background (Night)
        text: color!(0x9aa5ce),       // Text
        primary: color!(0x2ac3de),    // Blue
        success: color!(0x9ece6a),    // Green
        warning: color!(0xe0af68),    // Yellow
        danger: color!(0xf7768e),     // Red
    };

    /// The built-in [Tokyo Night] Storm variant of a [`Palette`].
    ///
    /// [Tokyo Night]: https://github.com/enkia/tokyo-night-vscode-theme
    pub const TOKYO_NIGHT_STORM: Self = Self {
        background: color!(0x24283b), // Background (Storm)
        text: color!(0x9aa5ce),       // Text
        primary: color!(0x2ac3de),    // Blue
        success: color!(0x9ece6a),    // Green
        warning: color!(0xe0af68),    // Yellow
        danger: color!(0xf7768e),     // Red
    };

    /// The built-in [Tokyo Night] Light variant of a [`Palette`].
    ///
    /// [Tokyo Night]: https://github.com/enkia/tokyo-night-vscode-theme
    pub const TOKYO_NIGHT_LIGHT: Self = Self {
        background: color!(0xd5d6db), // Background
        text: color!(0x565a6e),       // Text
        primary: color!(0x166775),    // Blue
        success: color!(0x485e30),    // Green
        warning: color!(0x8f5e15),    // Yellow
        danger: color!(0x8c4351),     // Red
    };

    /// The built-in [Kanagawa] Wave variant of a [`Palette`].
    ///
    /// [Kanagawa]: https://github.com/rebelot/kanagawa.nvim
    pub const KANAGAWA_WAVE: Self = Self {
        background: color!(0x1f1f28), // Sumi Ink 3
        text: color!(0xDCD7BA),       // Fuji White
        primary: color!(0x7FB4CA),    // Wave Blue
        success: color!(0x76946A),    // Autumn Green
        warning: color!(0xff9e3b),    // Ronin Yellow
        danger: color!(0xC34043),     // Autumn Red
    };

    /// The built-in [Kanagawa] Dragon variant of a [`Palette`].
    ///
    /// [Kanagawa]: https://github.com/rebelot/kanagawa.nvim
    pub const KANAGAWA_DRAGON: Self = Self {
        background: color!(0x181616), // Dragon Black 3
        text: color!(0xc5c9c5),       // Dragon White
        primary: color!(0x223249),    // Wave Blue 1
        success: color!(0x8a9a7b),    // Dragon Green 2
        warning: color!(0xff9e3b),    // Ronin Yellow
        danger: color!(0xc4746e),     // Dragon Red
    };

    /// The built-in [Kanagawa] Lotus variant of a [`Palette`].
    ///
    /// [Kanagawa]: https://github.com/rebelot/kanagawa.nvim
    pub const KANAGAWA_LOTUS: Self = Self {
        background: color!(0xf2ecbc), // Lotus White 3
        text: color!(0x545464),       // Lotus Ink 1
        primary: color!(0x4d699b),    // Lotus Blue
        success: color!(0x6f894e),    // Lotus Green
        warning: color!(0xe98a00),    // Lotus Orange 2
        danger: color!(0xc84053),     // Lotus Red
    };

    /// The built-in [Moonfly] variant of a [`Palette`].
    ///
    /// [Moonfly]: https://github.com/bluz71/vim-moonfly-colors
    pub const MOONFLY: Self = Self {
        background: color!(0x080808), // Background
        text: color!(0xbdbdbd),       // Foreground
        primary: color!(0x80a0ff),    // Blue (normal)
        success: color!(0x8cc85f),    // Green (normal)
        warning: color!(0xe3c78a),    // Yellow (normal)
        danger: color!(0xff5454),     // Red (normal)
    };

    /// The built-in [Nightfly] variant of a [`Palette`].
    ///
    /// [Nightfly]: https://github.com/bluz71/vim-nightfly-colors
    pub const NIGHTFLY: Self = Self {
        background: color!(0x011627), // Background
        text: color!(0xbdc1c6),       // Foreground
        primary: color!(0x82aaff),    // Blue (normal)
        success: color!(0xa1cd5e),    // Green (normal)
        warning: color!(0xe3d18a),    // Yellow (normal)
        danger: color!(0xfc514e),     // Red (normal)
    };

    /// The built-in [Oxocarbon] variant of a [`Palette`].
    ///
    /// [Oxocarbon]: https://github.com/nyoom-engineering/oxocarbon.nvim
    pub const OXOCARBON: Self = Self {
        background: color!(0x232323),
        text: color!(0xd0d0d0),
        primary: color!(0x00b4ff),
        success: color!(0x00c15a),
        warning: color!(0xbe95ff), // Base 14
        danger: color!(0xf62d0f),
    };

    /// The built-in [Ferra] variant of a [`Palette`].
    ///
    /// [Ferra]: https://github.com/casperstorm/ferra
    pub const FERRA: Self = Self {
        background: color!(0x2b292d),
        text: color!(0xfecdb2),
        primary: color!(0xd1d1e0),
        success: color!(0xb1b695),
        warning: color!(0xf5d76e), // Honey
        danger: color!(0xe06b75),
    };
}
