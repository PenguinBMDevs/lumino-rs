//! 主题与色板（本地实现，替代 `lumino_ui_core::geom::Theme`）。
//!
//! 22 个内置主题的色值与 旧版 0.14 逐字节一致（`palette::Extended::generate`
//! 的派生公式也原样保留），因此 egui 外壳的颜色与迁移前完全相同。
//!
//! 另外提供高对比度模式（High Contrast）：全黑底 + 白色字 + 黄色强调色。

pub mod palette;

pub use palette::{Extended as ExtendedPalette, Palette};

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::geom::Color;

/// 内置主题。
#[derive(Debug, Clone, PartialEq)]
pub enum Theme {
    /// 浅色。
    Light,
    /// 深色。
    Dark,
    /// Dracula。
    Dracula,
    /// Nord。
    Nord,
    /// Solarized Light。
    SolarizedLight,
    /// Solarized Dark。
    SolarizedDark,
    /// Gruvbox Light。
    GruvboxLight,
    /// Gruvbox Dark。
    GruvboxDark,
    /// Catppuccin Latte。
    CatppuccinLatte,
    /// Catppuccin Frappé。
    CatppuccinFrappe,
    /// Catppuccin Macchiato。
    CatppuccinMacchiato,
    /// Catppuccin Mocha。
    CatppuccinMocha,
    /// Tokyo Night。
    TokyoNight,
    /// Tokyo Night Storm。
    TokyoNightStorm,
    /// Tokyo Night Light。
    TokyoNightLight,
    /// Kanagawa Wave。
    KanagawaWave,
    /// Kanagawa Dragon。
    KanagawaDragon,
    /// Kanagawa Lotus。
    KanagawaLotus,
    /// Moonfly。
    Moonfly,
    /// Nightfly。
    Nightfly,
    /// Oxocarbon。
    Oxocarbon,
    /// Ferra。
    Ferra,
    /// 自定义色板。
    Custom(Arc<Custom>),
}

impl Theme {
    /// 全部内置主题。
    pub const ALL: &'static [Self] = &[
        Self::Light,
        Self::Dark,
        Self::Dracula,
        Self::Nord,
        Self::SolarizedLight,
        Self::SolarizedDark,
        Self::GruvboxLight,
        Self::GruvboxDark,
        Self::CatppuccinLatte,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::CatppuccinMocha,
        Self::TokyoNight,
        Self::TokyoNightStorm,
        Self::TokyoNightLight,
        Self::KanagawaWave,
        Self::KanagawaDragon,
        Self::KanagawaLotus,
        Self::Moonfly,
        Self::Nightfly,
        Self::Oxocarbon,
        Self::Ferra,
    ];

    /// 由给定色板创建自定义主题。
    pub fn custom(name: impl Into<Cow<'static, str>>, palette: Palette) -> Self {
        Self::Custom(Arc::new(Custom::with_fn(
            name,
            palette,
            palette::Extended::generate,
        )))
    }

    /// 返回主题色板。
    pub fn palette(&self) -> Palette {
        match self {
            Self::Light => Palette::LIGHT,
            Self::Dark => Palette::DARK,
            Self::Dracula => Palette::DRACULA,
            Self::Nord => Palette::NORD,
            Self::SolarizedLight => Palette::SOLARIZED_LIGHT,
            Self::SolarizedDark => Palette::SOLARIZED_DARK,
            Self::GruvboxLight => Palette::GRUVBOX_LIGHT,
            Self::GruvboxDark => Palette::GRUVBOX_DARK,
            Self::CatppuccinLatte => Palette::CATPPUCCIN_LATTE,
            Self::CatppuccinFrappe => Palette::CATPPUCCIN_FRAPPE,
            Self::CatppuccinMacchiato => Palette::CATPPUCCIN_MACCHIATO,
            Self::CatppuccinMocha => Palette::CATPPUCCIN_MOCHA,
            Self::TokyoNight => Palette::TOKYO_NIGHT,
            Self::TokyoNightStorm => Palette::TOKYO_NIGHT_STORM,
            Self::TokyoNightLight => Palette::TOKYO_NIGHT_LIGHT,
            Self::KanagawaWave => Palette::KANAGAWA_WAVE,
            Self::KanagawaDragon => Palette::KANAGAWA_DRAGON,
            Self::KanagawaLotus => Palette::KANAGAWA_LOTUS,
            Self::Moonfly => Palette::MOONFLY,
            Self::Nightfly => Palette::NIGHTFLY,
            Self::Oxocarbon => Palette::OXOCARBON,
            Self::Ferra => Palette::FERRA,
            Self::Custom(custom) => custom.palette,
        }
    }

    /// 返回派生色板（weak/strong/text 组合）。
    pub fn extended_palette(&self) -> &palette::Extended {
        match self {
            Self::Light => &palette::EXTENDED_LIGHT,
            Self::Dark => &palette::EXTENDED_DARK,
            Self::Dracula => &palette::EXTENDED_DRACULA,
            Self::Nord => &palette::EXTENDED_NORD,
            Self::SolarizedLight => &palette::EXTENDED_SOLARIZED_LIGHT,
            Self::SolarizedDark => &palette::EXTENDED_SOLARIZED_DARK,
            Self::GruvboxLight => &palette::EXTENDED_GRUVBOX_LIGHT,
            Self::GruvboxDark => &palette::EXTENDED_GRUVBOX_DARK,
            Self::CatppuccinLatte => &palette::EXTENDED_CATPPUCCIN_LATTE,
            Self::CatppuccinFrappe => &palette::EXTENDED_CATPPUCCIN_FRAPPE,
            Self::CatppuccinMacchiato => &palette::EXTENDED_CATPPUCCIN_MACCHIATO,
            Self::CatppuccinMocha => &palette::EXTENDED_CATPPUCCIN_MOCHA,
            Self::TokyoNight => &palette::EXTENDED_TOKYO_NIGHT,
            Self::TokyoNightStorm => &palette::EXTENDED_TOKYO_NIGHT_STORM,
            Self::TokyoNightLight => &palette::EXTENDED_TOKYO_NIGHT_LIGHT,
            Self::KanagawaWave => &palette::EXTENDED_KANAGAWA_WAVE,
            Self::KanagawaDragon => &palette::EXTENDED_KANAGAWA_DRAGON,
            Self::KanagawaLotus => &palette::EXTENDED_KANAGAWA_LOTUS,
            Self::Moonfly => &palette::EXTENDED_MOONFLY,
            Self::Nightfly => &palette::EXTENDED_NIGHTFLY,
            Self::Oxocarbon => &palette::EXTENDED_OXOCARBON,
            Self::Ferra => &palette::EXTENDED_FERRA,
            Self::Custom(custom) => &custom.extended,
        }
    }

    /// 主题名（设置列表与持久化共用）。
    pub fn name(&self) -> &str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Dracula => "Dracula",
            Self::Nord => "Nord",
            Self::SolarizedLight => "Solarized Light",
            Self::SolarizedDark => "Solarized Dark",
            Self::GruvboxLight => "Gruvbox Light",
            Self::GruvboxDark => "Gruvbox Dark",
            Self::CatppuccinLatte => "Catppuccin Latte",
            Self::CatppuccinFrappe => "Catppuccin Frappé",
            Self::CatppuccinMacchiato => "Catppuccin Macchiato",
            Self::CatppuccinMocha => "Catppuccin Mocha",
            Self::TokyoNight => "Tokyo Night",
            Self::TokyoNightStorm => "Tokyo Night Storm",
            Self::TokyoNightLight => "Tokyo Night Light",
            Self::KanagawaWave => "Kanagawa Wave",
            Self::KanagawaDragon => "Kanagawa Dragon",
            Self::KanagawaLotus => "Kanagawa Lotus",
            Self::Moonfly => "Moonfly",
            Self::Nightfly => "Nightfly",
            Self::Oxocarbon => "Oxocarbon",
            Self::Ferra => "Ferra",
            Self::Custom(custom) => &custom.name,
        }
    }
}

impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// 自定义主题（名称 + 色板 + 派生色板）。
#[derive(Debug, Clone, PartialEq)]
pub struct Custom {
    name: Cow<'static, str>,
    palette: Palette,
    extended: palette::Extended,
}

impl Custom {
    /// 由色板创建（派生色板按标准公式生成）。
    pub fn new(name: impl Into<Cow<'static, str>>, palette: Palette) -> Self {
        Self::with_fn(name, palette, palette::Extended::generate)
    }

    /// 由色板 + 自定义派生函数创建。
    pub fn with_fn(
        name: impl Into<Cow<'static, str>>,
        palette: Palette,
        generate: impl FnOnce(Palette) -> palette::Extended,
    ) -> Self {
        Self {
            name: name.into(),
            palette,
            extended: generate(palette),
        }
    }
}

impl fmt::Display for Custom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// 高对比度模式全局开关，被 ThemeExt 等 palette 路径使用。
static HIGH_CONTRAST: AtomicBool = AtomicBool::new(false);

/// 高对比度主题在设置列表中的显示名称。
pub const HIGH_CONTRAST_DISPLAY: &str = "高对比度 (High Contrast)";

/// 启用/禁用高对比度模式
pub fn set_high_contrast(enabled: bool) {
    HIGH_CONTRAST.store(enabled, Ordering::SeqCst);
}

/// 检查当前是否在高对比度模式
pub fn is_high_contrast() -> bool {
    HIGH_CONTRAST.load(Ordering::SeqCst)
}

/// 构建高对比度 Custom Theme。
///
/// 使用 `Theme::custom` 包装全黑 palette，使 `theme.palette()` /
/// `theme.extended_palette()` 调用自动返回黑色系颜色。
pub fn hc_theme() -> Theme {
    Theme::custom(
        HIGH_CONTRAST_DISPLAY,
        Palette {
            background: Color::BLACK,                // 纯黑底
            text: Color::WHITE,                      // 纯白字
            primary: Color::from_rgb(1.0, 0.8, 0.0), // 黄色强调
            success: Color::from_rgb(0.0, 0.85, 0.2),
            warning: Color::from_rgb(1.0, 0.8, 0.0), // 同强调色
            danger: Color::from_rgb(0.9, 0.15, 0.15),
        },
    )
}

/// 高对比度色板 — "用心看世界"，纯黑底黄绿图标
pub mod hc {
    use crate::geom::Color;

    /// 纯黑底
    pub const BG: Color = Color::BLACK;
    /// 编码器背景块（采样区/键盘区域边界）
    pub const KEYBOARD_BG: Color = Color::from_rgb(0.04, 0.04, 0.04);
    /// 尺规背景
    pub const RULER_BG: Color = Color::from_rgb(0.06, 0.06, 0.06);

    /// 白色文字/前景
    pub const TEXT: Color = Color::WHITE;
    /// 白键 — 浅灰色系防止与全黑背景不可见
    pub const WHITE_KEY: Color = Color::from_rgb(0.12, 0.12, 0.12);
    /// 黑键 — 纯黑
    pub const BLACK_KEY: Color = Color::from_rgb(0.02, 0.02, 0.02);

    /// 黄色强调 — 我最懂你。HHKB / 专业 DAW 标志色
    pub const ACCENT: Color = Color::from_rgb(1.0, 0.8, 0.0);

    /// 网格线
    pub const GRID_LINE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.12);
    /// 半拍线
    pub const HALF_BEAT_LINE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.30);
    /// 拍线
    pub const BEAT_LINE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.50);
    /// 小节线
    pub const BAR_LINE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.85);

    /// 通用边框
    pub const BORDER: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.6);
    /// 白键分隔线
    pub const KEY_LINE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.35);
}
