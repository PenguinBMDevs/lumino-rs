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
use std::sync::LazyLock;
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

/// 内置主题表项：`(枚举值, 持久化名, 色板, 派生色板)`。
type Builtin = (Theme, &'static str, Palette, LazyLock<palette::Extended>);

/// 表项简写：只写枚举值/名称/色板，派生色板按标准公式惰性生成。
///
/// 这不是主题注册宏——`Theme::ALL` 仍是唯一的枚举清单，本表只是把
/// 「名称 + 色板 + 派生色板」放在同一行，避免 22 个 `EXTENDED_*` 静态。
macro_rules! builtin {
    ($theme:ident, $name:literal, $palette:ident) => {
        (
            Theme::$theme,
            $name,
            Palette::$palette,
            LazyLock::new(|| palette::Extended::generate(Palette::$palette)),
        )
    };
}

/// 内置主题单一表（顺序必须与 [`Theme::ALL`] 一致，由单测锁定）。
///
/// 原先 `palette` / `extended_palette` / `name` 各有一份 22 分支 match，
/// 派生色板另有 22 个 `EXTENDED_*` 静态；改为本表后新增主题只需在
/// [`Theme::ALL`] 与本表各加一行。
static BUILTINS: [Builtin; 22] = [
    builtin!(Light, "Light", LIGHT),
    builtin!(Dark, "Dark", DARK),
    builtin!(Dracula, "Dracula", DRACULA),
    builtin!(Nord, "Nord", NORD),
    builtin!(SolarizedLight, "Solarized Light", SOLARIZED_LIGHT),
    builtin!(SolarizedDark, "Solarized Dark", SOLARIZED_DARK),
    builtin!(GruvboxLight, "Gruvbox Light", GRUVBOX_LIGHT),
    builtin!(GruvboxDark, "Gruvbox Dark", GRUVBOX_DARK),
    builtin!(CatppuccinLatte, "Catppuccin Latte", CATPPUCCIN_LATTE),
    builtin!(CatppuccinFrappe, "Catppuccin Frappé", CATPPUCCIN_FRAPPE),
    builtin!(
        CatppuccinMacchiato,
        "Catppuccin Macchiato",
        CATPPUCCIN_MACCHIATO
    ),
    builtin!(CatppuccinMocha, "Catppuccin Mocha", CATPPUCCIN_MOCHA),
    builtin!(TokyoNight, "Tokyo Night", TOKYO_NIGHT),
    builtin!(TokyoNightStorm, "Tokyo Night Storm", TOKYO_NIGHT_STORM),
    builtin!(TokyoNightLight, "Tokyo Night Light", TOKYO_NIGHT_LIGHT),
    builtin!(KanagawaWave, "Kanagawa Wave", KANAGAWA_WAVE),
    builtin!(KanagawaDragon, "Kanagawa Dragon", KANAGAWA_DRAGON),
    builtin!(KanagawaLotus, "Kanagawa Lotus", KANAGAWA_LOTUS),
    builtin!(Moonfly, "Moonfly", MOONFLY),
    builtin!(Nightfly, "Nightfly", NIGHTFLY),
    builtin!(Oxocarbon, "Oxocarbon", OXOCARBON),
    builtin!(Ferra, "Ferra", FERRA),
];

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

    /// 查找内置主题描述；`Custom` 与未登记变体返回 `None`。
    fn builtin(&self) -> Option<&'static Builtin> {
        BUILTINS.iter().find(|(theme, ..)| theme == self)
    }

    /// 返回主题色板。
    pub fn palette(&self) -> Palette {
        if let Self::Custom(custom) = self {
            return custom.palette;
        }
        match self.builtin() {
            Some((_, _, palette, _)) => *palette,
            None => {
                // 所有非 Custom 变体都应在表中；debug 构建直接暴露漏登记
                debug_assert!(false, "内置主题缺少 BUILTINS 表项: {self:?}");
                Palette::DARK
            }
        }
    }

    /// 返回派生色板（weak/strong/text 组合）。
    pub fn extended_palette(&self) -> &palette::Extended {
        if let Self::Custom(custom) = self {
            return &custom.extended;
        }
        match self.builtin() {
            Some((_, _, _, extended)) => extended,
            None => {
                debug_assert!(false, "内置主题缺少 BUILTINS 表项: {self:?}");
                &BUILTINS[1].3
            }
        }
    }

    /// 主题名（设置列表与持久化共用）。
    pub fn name(&self) -> &str {
        if let Self::Custom(custom) = self {
            return &custom.name;
        }
        match self.builtin() {
            Some((_, name, _, _)) => name,
            None => {
                debug_assert!(false, "内置主题缺少 BUILTINS 表项: {self:?}");
                "Unknown"
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 单一表必须与 `Theme::ALL` 一一对应（数量与顺序）
    #[test]
    fn builtin_table_covers_all_themes_in_order() {
        assert_eq!(
            BUILTINS.len(),
            Theme::ALL.len(),
            "表项与 Theme::ALL 数量不一致"
        );
        for ((builtin, ..), theme) in BUILTINS.iter().zip(Theme::ALL) {
            assert_eq!(builtin, theme, "表顺序必须与 Theme::ALL 一致");
        }
    }

    /// 三条访问路径都必须命中表项（防止 getter 回退到兜底值）
    #[test]
    fn theme_getters_resolve_from_table() {
        for (theme, name, palette, extended) in BUILTINS.iter() {
            assert_eq!(theme.name(), *name);
            assert_eq!(theme.palette(), *palette);
            assert_eq!(*theme.extended_palette(), **extended);
        }
    }

    /// Custom 主题不受表影响
    #[test]
    fn custom_theme_uses_own_values() {
        let theme = Theme::custom("Custom Test", Palette::LIGHT);
        assert_eq!(theme.name(), "Custom Test");
        assert_eq!(theme.palette(), Palette::LIGHT);
        assert_eq!(
            *theme.extended_palette(),
            palette::Extended::generate(Palette::LIGHT)
        );
    }
}
