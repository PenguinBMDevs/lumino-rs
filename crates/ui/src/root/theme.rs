//! 主题系统扩展
//!
//! 提供高对比度模式 (High Contrast) 支持。
//! 高对比度模式下，使用 `iced_core::Theme::Custom` 创建全黑底 + 纯白字 + 金色点缀的色板，
//! 使得所有 Iced 内置 widget 的 `palette()` / `extended_palette()` 调用自动返回黑色系颜色，
//! 无需逐个覆盖 style 闭包。
//!
//! "这个主题懂得都懂" — 经典的 #000000 + #FFCC00 配色，
//! 黑底金标，线条一清二楚。

use crate::Theme;
use iced_core::Color;
use std::sync::atomic::{AtomicBool, Ordering};

/// 高对比度模式全局开关（供 ThemeExt 等非 palette 路径使用）
static HIGH_CONTRAST: AtomicBool = AtomicBool::new(false);

/// 高对比度主题在主题选择列表中的显示名称
pub const HIGH_CONTRAST_DISPLAY: &str = "高对比度 (High Contrast)";

/// 启用/禁用高对比度模式
pub fn set_high_contrast(enabled: bool) {
    HIGH_CONTRAST.store(enabled, Ordering::SeqCst);
}

/// 检查当前是否处于高对比度模式
pub fn is_high_contrast() -> bool {
    HIGH_CONTRAST.load(Ordering::SeqCst)
}

/// 创建高对比度 Custom Theme。
///
/// 使用 `Theme::Custom` 包装全黑 palette，所有 `theme.palette()` /
/// `theme.extended_palette()` 调用自动返回黑色系颜色，
/// Iced 内建 widget（container / text / button 等）无需任何修改即呈现黑底。
pub fn hc_theme() -> Theme {
    Theme::custom(
        HIGH_CONTRAST_DISPLAY,
        iced_core::theme::palette::Palette {
            background: Color::BLACK,                // 纯黑背景
            text: Color::WHITE,                      // 纯白文字
            primary: Color::from_rgb(1.0, 0.8, 0.0), // 金色交互色
            success: Color::from_rgb(0.0, 0.85, 0.2),
            warning: Color::from_rgb(1.0, 0.8, 0.0), // 同金色
            danger: Color::from_rgb(0.9, 0.15, 0.15),
        },
    )
}

/// 高对比度色板 — "懂的都懂"经典黑底金标配色
pub mod hc {
    use iced_core::Color;

    /// 纯黑背景
    pub const BG: Color = Color::BLACK;
    /// 键盘背景区（比纯黑略亮以区分边界）
    pub const KEYBOARD_BG: Color = Color::from_rgb(0.04, 0.04, 0.04);
    /// 标尺背景
    pub const RULER_BG: Color = Color::from_rgb(0.06, 0.06, 0.06);

    /// 纯白文字/前景
    pub const TEXT: Color = Color::WHITE;
    /// 白键 — 深色系，防止刺眼且让文本可见
    pub const WHITE_KEY: Color = Color::from_rgb(0.12, 0.12, 0.12);
    /// 黑键 — 极黑
    pub const BLACK_KEY: Color = Color::from_rgb(0.02, 0.02, 0.02);

    /// 金色点缀 — 懂的都懂，HHKB / 专业 DAW 标志色
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
    /// 琴键分隔线
    pub const KEY_LINE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.35);
}
