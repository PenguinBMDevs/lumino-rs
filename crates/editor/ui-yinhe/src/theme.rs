//! yinhe 主题映射 — P1 已废弃 yinhe 自有主题，统一走 lumino Theme
//!
//! 背景与决策：
//! - `yinhe/crates/yinhe-theme/src/*.rs` 曾定义 `YinheTheme { base: BaseColors, accent, warning, danger }`
//!   与 egui `Visuals` 绑定；`yinhe/crates/yinhe-egui/src/theme/*`（`colors.rs`, `visuals.rs`,
//!   `style.rs` 等）提供 egui 侧的硬编码色板与样式覆盖。
//! - lumino 主题路径：`crates/editor/ui-core/src/theme.rs:35` 的 `hc_theme()` 使用
//!   `iced_core::Theme::Custom(iced_core::theme::palette::Palette { background, text, primary, ... })`，
//!   通过 `theme.palette()` / `extended_palette()` 自动作用于所有 iced 内置 widget，无需样式模板侵入。
//! - P1 已定：yinhe 主题 **按 lumino 主题走** — `lumino-ui-yinhe` 不自建 `Theme` 枚举，
//!   直接复用 `lumino_ui_core::Theme`（即 `iced_core::Theme`）；字体复用
//!   `crates/editor/ui/src/host.rs:260` 的 `create_font_from_config`（读取
//!   `UiConfig.program_font_name/path`，`crates/core/core/src/storage/config.rs:172 UiConfig 42字段`），
//!   图标统一走 SVG `lumino_ui_core::resources::icon::view_with_size_and_theme`（`define_icons!`）。
//! - 因此 `yinhe base / colors` **已废弃**：不再参与渲染，仅保留 **数值迁移** 能力，
//!   将旧 `config.json` / `yinhe_layout.json` 中残留的 yinhe RGB 数值一次性映射到
//!   lumino `Palette`，便于配置兼容与平滑迁移。
//!
//! 字体/图标约束（P1 验证项）：
//! - 本 crate 所有 `view` 不硬编码 `Font::with_name(_)`，一律使用 `Theme` 默认字体
//!   （`Font::DEFAULT` / `Font::MONOSPACE` 或不指定，交由 `Theme` 决定；如需用户字体，
//!   由宿主 `Host` 通过 `UiConfig` 统一注入，与 lumino `host.rs:260` 一致）。
//! - `Cargo.toml` 不引入新的字体包（`font-kit` / `ab_glyph` / `ttf-parser` 等仅由宿主持有）。

use iced_core::Color;
use iced_core::theme::palette::Palette;
use lumino_ui_core::Theme;

/// 已废弃的 yinhe base 色板 — 仅用于旧配置数值迁移
///
/// 对应 `yinhe-theme/src/base.rs` 曾定义的 `Base { bg, fg, panel, border, ... }`。
/// P1 后不再用于渲染；保留结构与字段仅为 **数值搬运**，最终映射到
/// `Palette { background, text, primary, ... }`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeprecatedYinheBase {
    /// 背景色（yinhe `base.bg` / `colors.background`）
    pub background: Color,
    /// 前景/文字色（yinhe `base.fg` / `colors.text`）
    pub foreground: Color,
    /// 面板/卡片色（yinhe `base.panel`）
    pub panel: Color,
    /// 强调色（yinhe `base.accent` / `colors.accent`）
    pub accent: Color,
}

/// 已废弃的 yinhe 扩展色 — 对应 `yinhe-theme/src/colors.rs` 的扩展色表
///
/// P1 后扩展色亦废弃，若旧配置仍以数值形式出现，则通过
/// [`map_yinhe_colors_to_palette`] 映射到 `Palette` 的 `success / warning / danger`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeprecatedYinheColors {
    /// 基础底色
    pub base: DeprecatedYinheBase,
    /// 成功/完成色
    pub success: Color,
    /// 警告色
    pub warning: Color,
    /// 危险/错误色
    pub danger: Color,
}

impl Default for DeprecatedYinheBase {
    fn default() -> Self {
        Self {
            background: Color::from_rgb(0.12, 0.12, 0.12),
            foreground: Color::from_rgb(0.93, 0.93, 0.93),
            panel: Color::from_rgb(0.18, 0.18, 0.18),
            accent: Color::from_rgb(0.20, 0.60, 1.0),
        }
    }
}

impl Default for DeprecatedYinheColors {
    fn default() -> Self {
        Self {
            base: DeprecatedYinheBase::default(),
            success: Color::from_rgb(0.0, 0.85, 0.20),
            warning: Color::from_rgb(1.0, 0.8, 0.0),
            danger: Color::from_rgb(0.9, 0.15, 0.15),
        }
    }
}

/// 将已废弃的 yinhe `Base` 数值映射到 lumino `Theme::Custom` palette
///
/// 注释：`yinhe base / colors 已废弃，仅保留数值映射到 lumino palette` — 本函数
/// 不再创建 yinhe 自有主题，而是把旧 RGB 数值搬运到 `Palette` 字段，交由
/// `Theme::custom` 包装，使 `theme.palette()` / `extended_palette()` 后续调用
/// 自动返回迁移后的 lumino 色板。
///
/// 映射规则（与 `ui-core/src/theme.rs:35 hc_theme()` 的 `Palette` 布局对齐）：
/// - `base.background` → `Palette.background`
/// - `base.foreground` → `Palette.text`
/// - `base.accent`     → `Palette.primary`（兼 `warning` 若未单独指定）
/// - `base.panel`      → 丢弃（lumino 由 `Extended` 自动派生 weak/strong，不再需 panel）
/// - `success/warning/danger` 若来自 `DeprecatedYinheColors` 则透传，否则回落默认值
#[must_use]
pub fn map_yinhe_base_to_lumino(base: &DeprecatedYinheBase) -> Theme {
    map_yinhe_base_with_overrides_to_lumino(base, None, None, None)
}

/// 带覆盖色的 `map_yinhe_base_to_lumino` 变体（供 `DeprecatedYinheColors` 调用）
///
/// `success / warning / danger` 为 `None` 时回落到 lumino 默认或 `base.accent` 派生。
#[must_use]
pub fn map_yinhe_base_with_overrides_to_lumino(
    base: &DeprecatedYinheBase,
    success: Option<Color>,
    warning: Option<Color>,
    danger: Option<Color>,
) -> Theme {
    Theme::custom(
        "yinhe-migrated",
        Palette {
            background: base.background,
            text: base.foreground,
            primary: base.accent,
            success: success.unwrap_or(Color::from_rgb(0.0, 0.85, 0.20)),
            warning: warning.unwrap_or(base.accent),
            danger: danger.unwrap_or(Color::from_rgb(0.9, 0.15, 0.15)),
        },
    )
}

/// 将已废弃的 yinhe 扩展色表整体映射到 lumino `Palette` 的 `Theme`
///
/// 等价于 `map_yinhe_base_to_lumino` + 扩展色透传，是 `yinhe-egui/src/theme/colors.rs`
/// 中 `YinheColors { base, success, warning, danger }` 的迁移入口。
#[must_use]
pub fn map_yinhe_colors_to_palette(colors: &DeprecatedYinheColors) -> Theme {
    map_yinhe_base_with_overrides_to_lumino(
        &colors.base,
        Some(colors.success),
        Some(colors.warning),
        Some(colors.danger),
    )
}

/// 将 yinhe 旧主题预设名映射到 lumino 主题名
///
/// yinhe 曾有 `Dark / Light / HighContrast` 等预设；P1 后预设名由
/// `UiConfig.theme: String`（42字段之一）统一管理，查询 `Theme::ALL` 与
/// `HIGH_CONTRAST_DISPLAY`（`crates/editor/ui-core/src/theme.rs:35`）。
/// 本函数仅做 **字符串数值映射**，不创建新主题：
///
/// | yinhe 旧名 | lumino 目标 |
/// |---|---|---|
/// | `yinhe_dark` / `dark` | `Tokyo Night Storm`（lumino 默认暗色） |
/// | `yinhe_light` / `light` | `Light` |
/// | `yinhe_hc` / `high_contrast` | `高对比度 (High Contrast)` |
/// | 其他 | `None`（调用方回落到 `Window::default_theme()`）|
#[must_use]
pub fn map_yinhe_theme_name_to_lumino(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "yinhe_dark" | "dark" | "yinhe-dark" => Some("Tokyo Night Storm"),
        "yinhe_light" | "light" | "yinhe-light" => Some("Light"),
        "yinhe_hc" | "high_contrast" | "high-contrast" | "hc" => {
            Some(lumino_ui_core::theme::HIGH_CONTRAST_DISPLAY)
        }
        _ => None,
    }
}

/// 将旧 `yinhe_layout.json` 中以 0xRRGGBB 数值存储的颜色映射为 `Color`
///
/// yinhe 曾以 `u32` 存储 `0xRRGGBB`；lumino `Palette` 使用 `Color::from_rgb*`。
/// 本函数保留数值转换路径，避免旧文件解析失败。
#[must_use]
pub fn yinhe_u32_to_color(rgb: u32) -> Color {
    let r = ((rgb >> 16) & 0xFF) as u8;
    let g = ((rgb >> 8) & 0xFF) as u8;
    let b = (rgb & 0xFF) as u8;
    Color::from_rgb8(r, g, b)
}

/// 反向：`Color` → `0xRRGGBB`（用于需要回写旧格式的兼容路径，P1 默认不调用）
#[must_use]
pub fn color_to_yinhe_u32(color: Color) -> u32 {
    let r = (color.r * 255.0).round().clamp(0.0, 255.0) as u32;
    let g = (color.g * 255.0).round().clamp(0.0, 255.0) as u32;
    let b = (color.b * 255.0).round().clamp(0.0, 255.0) as u32;
    (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_base_to_lumino_preserves_numbers() {
        let base = DeprecatedYinheBase {
            background: Color::from_rgb(0.10, 0.20, 0.30),
            foreground: Color::WHITE,
            panel: Color::BLACK,
            accent: Color::from_rgb(1.0, 0.0, 0.0),
        };
        let theme = map_yinhe_base_to_lumino(&base);
        let palette = theme.palette();
        assert_eq!(palette.background, base.background);
        assert_eq!(palette.text, base.foreground);
        assert_eq!(palette.primary, base.accent);
    }

    #[test]
    fn map_colors_to_palette() {
        let colors = DeprecatedYinheColors {
            base: DeprecatedYinheBase {
                background: Color::BLACK,
                foreground: Color::WHITE,
                panel: Color::BLACK,
                accent: Color::from_rgb(1.0, 0.8, 0.0),
            },
            success: Color::from_rgb(0.0, 1.0, 0.0),
            warning: Color::from_rgb(1.0, 1.0, 0.0),
            danger: Color::from_rgb(1.0, 0.0, 0.0),
        };
        let theme = map_yinhe_colors_to_palette(&colors);
        let p = theme.palette();
        assert_eq!(p.success, colors.success);
        assert_eq!(p.warning, colors.warning);
        assert_eq!(p.danger, colors.danger);
    }

    #[test]
    fn theme_name_mapping() {
        assert_eq!(
            map_yinhe_theme_name_to_lumino("yinhe_dark"),
            Some("Tokyo Night Storm")
        );
        assert_eq!(map_yinhe_theme_name_to_lumino("light"), Some("Light"));
        assert_eq!(
            map_yinhe_theme_name_to_lumino("high_contrast"),
            Some(lumino_ui_core::theme::HIGH_CONTRAST_DISPLAY)
        );
        assert_eq!(map_yinhe_theme_name_to_lumino("unknown_preset"), None);
    }

    #[test]
    fn u32_color_roundtrip() {
        let rgb = 0x336699_u32;
        let c = yinhe_u32_to_color(rgb);
        let back = color_to_yinhe_u32(c);
        assert_eq!(back, rgb);
    }
}
