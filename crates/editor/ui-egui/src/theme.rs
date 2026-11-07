//! egui 主题：Tokyo Night Storm 的 Lumino 派生色板。
//!
//! 色值不是采样近似值，而是直接由 iced `Theme::TokyoNightStorm.extended_palette()`
//! 导出的精确值（见 `crates/editor/ui-parity/src/bin/dump_palette.rs`）。

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, Style, TextStyle, Visuals};

/// 窗口 / 标题栏底色（iced `background.neutral`）。
pub const TITLEBAR_BG: Color32 = Color32::from_rgb(0x3F, 0x46, 0x6E);
/// 工具栏分组 / 状态栏底色（iced `background.weak`）。
pub const TOOLBAR_BG: Color32 = Color32::from_rgb(0x39, 0x40, 0x63);
/// 工具栏底 / 左侧面板底色（iced `background.weakest`）。
pub const PANEL_BG: Color32 = Color32::from_rgb(0x2A, 0x2F, 0x47);
/// 左右路由栏底色（iced `background.weaker`）。
pub const ROUTE_BAR_BG: Color32 = Color32::from_rgb(0x33, 0x39, 0x57);
/// 编辑器 / 对话框内容底色（iced `background.base`）。
pub const BASE_BG: Color32 = Color32::from_rgb(0x24, 0x28, 0x3B);
/// 卡片底色（与内容底同色，靠边框区分）。
pub const CARD_BG: Color32 = BASE_BG;
/// 选中行 / 悬停按钮底色（iced `background.strong`）。
pub const SELECTED_BG: Color32 = Color32::from_rgb(0x44, 0x4D, 0x79);
/// 通用边框。
pub const BORDER: Color32 = Color32::from_rgb(0x3F, 0x45, 0x5C);
/// 正文（iced `background.base.text`）。
pub const TEXT: Color32 = Color32::from_rgb(0xB6, 0xC4, 0xFA);
/// 次级文字。
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x96, 0xA3, 0xBD);
/// `background.weak` 上的正文（iced `background.weak.text`）。
pub const TEXT_ON_WEAK: Color32 = Color32::from_rgb(0xD2, 0xE4, 0xFF);
/// 更弱的文字 / 分隔符。
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x7A, 0x82, 0xA8);
/// 强调色（iced `primary.base`）。
pub const ACCENT: Color32 = Color32::from_rgb(0x2A, 0xC3, 0xDE);
/// 强调色强态（iced `primary.strong`，状态栏数值用它）。
pub const ACCENT_STRONG: Color32 = Color32::from_rgb(0x00, 0xA3, 0xBD);
/// 强调色按下态。
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0x28, 0x9D, 0xB4);
/// 成功 / 绿。
pub const SUCCESS: Color32 = Color32::from_rgb(0x9E, 0xCE, 0x6A);
/// 警告 / 黄。
pub const WARNING: Color32 = Color32::from_rgb(0xE0, 0xAF, 0x68);
/// 危险 / 红。
pub const DANGER: Color32 = Color32::from_rgb(0xF7, 0x76, 0x8E);
/// 录制红点。
pub const RECORD_RED: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);
/// 播放指示线（iced `Color::from_rgb(1.0, 0.2, 0.2)`）。
pub const PLAYHEAD: Color32 = Color32::from_rgb(0xFF, 0x33, 0x33);
/// 左侧 / 右侧路由栏激活指示条（钢琴卷帘分组色）。
pub const ROUTE_INDICATOR: Color32 = Color32::from_rgb(0xA6, 0x59, 0x59);
/// Windows 关闭按钮 hover 红。
pub const CLOSE_HOVER: Color32 = Color32::from_rgb(0xC4, 0x2B, 0x1C);

/// 字体尺寸（逻辑点）。
pub mod font {
    /// 状态栏 / 小字。
    pub const SMALL: f32 = 11.0;
    /// 正文 / 按钮。
    pub const BODY: f32 = 13.0;
    /// 菜单 / 设置标签。
    pub const LABEL: f32 = 14.0;
    /// 标题。
    pub const TITLE: f32 = 18.0;
    /// 章节标题。
    pub const SECTION: f32 = 16.0;
}

/// 应用主题到 egui Context。
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_global_style(style());
    ctx.set_visuals(visuals());
}

/// 生成 Lumino 的 egui `Visuals`。
pub fn visuals() -> Visuals {
    let mut visuals = Visuals::dark();

    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BASE_BG;
    visuals.window_fill = BASE_BG;
    visuals.extreme_bg_color = BASE_BG;
    visuals.faint_bg_color = PANEL_BG;
    visuals.code_bg_color = PANEL_BG;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARNING;
    visuals.error_fg_color = DANGER;

    visuals.widgets.noninteractive.bg_fill = BASE_BG;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(4);

    visuals.widgets.inactive.bg_fill = TOOLBAR_BG;
    visuals.widgets.inactive.weak_bg_fill = TOOLBAR_BG;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x47, 0x4F, 0x7A);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x47, 0x4F, 0x7A);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x55, 0x5E, 0x8F));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

    visuals.widgets.active.bg_fill = Color32::from_rgb(0x55, 0x5E, 0x8F);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0x55, 0x5E, 0x8F);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_DARK);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, AccentText::value());
    visuals.widgets.active.corner_radius = CornerRadius::same(4);

    visuals.widgets.open.bg_fill = TOOLBAR_BG;
    visuals.widgets.open.weak_bg_fill = TOOLBAR_BG;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.open.corner_radius = CornerRadius::same(4);

    visuals
}

/// 强调色按钮上的文字（深底色，保证对比度）。
struct AccentText;

impl AccentText {
    const fn value() -> Color32 {
        Color32::from_rgb(0x12, 0x1A, 0x2A)
    }
}

/// 生成 Lumino 的 egui `Style`。
pub fn style() -> Style {
    let mut style = Style::default();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size = egui::vec2(24.0, 24.0);
    style.spacing.window_margin = egui::Margin::same(0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.indent = 14.0;
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(font::SMALL)),
        (TextStyle::Body, FontId::proportional(font::BODY)),
        (TextStyle::Button, FontId::proportional(font::BODY)),
        (TextStyle::Heading, FontId::proportional(font::TITLE)),
        (TextStyle::Monospace, FontId::monospace(font::SMALL)),
    ]
    .into();
    style
}

/// 文本样式快捷构造（供局部覆盖）。
pub fn text_style(size: f32, color: Color32) -> TextStyle {
    let _ = color;
    TextStyle::Name(format!("lumino-text-{size}").into())
}

/// 字体族：优先使用注入的 CJK 比例字体。
pub fn proportional() -> FontFamily {
    FontFamily::Proportional
}
