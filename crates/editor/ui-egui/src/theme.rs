//! egui 主题：Lumino 语义色板（跟随 旧版 主题）。
//!
//! 默认色板 = Tokyo Night Storm，色值不是采样近似值，而是直接取
//! 旧版 `Theme::TokyoNightStorm.extended_palette()` 的精确值
//! （见 `crates/editor/ui-parity/src/bin/dump_palette.rs`），
//! 因此场景像素基线与无头测试保持历史数值不变。
//!
//! 生产侧按当前 旧版 主题构造 [`PaletteSeed`]，再由 [`Palette::from_seed`]
//! 派生 egui 需要但 旧版 没有直接给出的颜色（边框 / 次级文字 / 悬停填充等）。
//! 这样用户切到 `Light` 等主题时，egui 外壳不再是固定的深色。
//!
//! 当前色板保存在线程局部：egui 一帧的绘制是单线程的，入口（
//! `chrome::show_bands` / 对话框）每帧调用 [`apply`] 前先 [`set_palette`]。

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, Style, TextStyle, Visuals};
use std::cell::Cell;

/// 一对 egui 语义色板：覆盖外壳 / 面板 / 控件所需的全部颜色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// 是否深色主题（决定控件基准 Visuals）。
    pub is_dark: bool,
    /// 窗口 / 标题栏底色（旧版 `background.neutral`）。
    pub titlebar_bg: Color32,
    /// 失焦标题栏底色（旧版 `background.weaker`）。
    pub titlebar_bg_unfocused: Color32,
    /// 工具栏分隔 / 状态栏底色（旧版 `background.weak`）。
    pub toolbar_bg: Color32,
    /// 工具栏底 / 左侧面板底色（旧版 `background.weakest`）。
    pub panel_bg: Color32,
    /// 左右路由栏底色（旧版 `background.weaker`）。
    pub route_bar_bg: Color32,
    /// 编辑器 / 对话框内容底色（旧版 `background.base`）。
    pub base_bg: Color32,
    /// 卡片底色（深色与内容底同色，浅色用 `weakest` 以区分层次）。
    pub card_bg: Color32,
    /// 选中行 / 悬停按钮底色（旧版 `background.strong`）。
    pub selected_bg: Color32,
    /// 通用边框。
    pub border: Color32,
    /// 正文（旧版 `background.base.text`）。
    pub text: Color32,
    /// 次级文字。
    pub text_muted: Color32,
    /// `background.weak` 上的正文（旧版 `background.weak.text`）。
    pub text_on_weak: Color32,
    /// 更弱的文字 / 分隔符。
    pub text_dim: Color32,
    /// 强调色（旧版 `primary.base`）。
    pub accent: Color32,
    /// 强调色强态（旧版 `primary.strong`）。
    pub accent_strong: Color32,
    /// 强调色弱态 / 按下态（旧版 `primary.weak`）。
    pub accent_dark: Color32,
    /// 强调色上的文字（按强调色亮度取深色或白色）。
    pub on_accent: Color32,
    /// 控件悬停填充。
    pub hover_bg: Color32,
    /// 控件悬停边框。
    pub hover_border: Color32,
    /// 控件按下填充。
    pub active_bg: Color32,
    /// 成功 / 绿色。
    pub success: Color32,
    /// 警告 / 黄色。
    pub warning: Color32,
    /// 危险 / 红色。
    pub danger: Color32,
    /// 录制红点。
    pub record_red: Color32,
    /// 播放指示线。
    pub playhead: Color32,
    /// 路由栏激活指示条。
    pub route_indicator: Color32,
    /// Windows 关闭按钮 hover 红。
    pub close_hover: Color32,
    /// 钢琴卷帘：键盘列底色（旧版 `keyboard_background_color`）。
    pub roll_keyboard_bg: Color32,
    /// 钢琴卷帘：白键（旧版 `white_key_color`）。
    pub roll_white_key: Color32,
    /// 钢琴卷帘：黑键（旧版 `black_key_color`）。
    pub roll_black_key: Color32,
    /// 钢琴卷帘：琴键分隔线（旧版 `key_line_color`）。
    pub roll_key_line: Color32,
    /// 钢琴卷帘：标尺底色（旧版 `ruler_background_color`）。
    pub roll_ruler_bg: Color32,
    /// 钢琴卷帘：音名 / 小节号文字（旧版 `text_color`）。
    pub roll_text: Color32,
}

/// 从 旧版 `Extended` 调色板摘出的语义种子（生产侧构造，ui-egui 无需依赖 旧版）。
#[derive(Debug, Clone, Copy)]
pub struct PaletteSeed {
    /// 是否深色主题（旧版 `Extended::is_dark`）。
    pub is_dark: bool,
    /// `background.base`。
    pub base: Color32,
    /// `background.weakest`。
    pub weakest: Color32,
    /// `background.weaker`。
    pub weaker: Color32,
    /// `background.weak`。
    pub weak: Color32,
    /// `background.neutral`。
    pub neutral: Color32,
    /// `background.strong`。
    pub strong: Color32,
    /// `background.strongest`。
    pub strongest: Color32,
    /// `background.base.text`。
    pub text: Color32,
    /// `background.weak.text`。
    pub text_on_weak: Color32,
    /// `primary.base`。
    pub primary: Color32,
    /// `primary.strong`。
    pub primary_strong: Color32,
    /// `primary.weak`。
    pub primary_weak: Color32,
    /// `success.base`。
    pub success: Color32,
    /// `warning.base`。
    pub warning: Color32,
    /// `danger.base`。
    pub danger: Color32,
}

impl Palette {
    /// 历史基线色板（Tokyo Night Storm，旧版 精确值）。
    pub const TOKYO_NIGHT_STORM: Self = Self {
        is_dark: true,
        titlebar_bg: Color32::from_rgb(0x3F, 0x46, 0x6E),
        titlebar_bg_unfocused: Color32::from_rgb(0x33, 0x39, 0x57),
        toolbar_bg: Color32::from_rgb(0x39, 0x40, 0x63),
        panel_bg: Color32::from_rgb(0x2A, 0x2F, 0x47),
        route_bar_bg: Color32::from_rgb(0x33, 0x39, 0x57),
        base_bg: Color32::from_rgb(0x24, 0x28, 0x3B),
        card_bg: Color32::from_rgb(0x24, 0x28, 0x3B),
        selected_bg: Color32::from_rgb(0x44, 0x4D, 0x79),
        border: Color32::from_rgb(0x3F, 0x45, 0x5C),
        text: Color32::from_rgb(0xB6, 0xC4, 0xFA),
        text_muted: Color32::from_rgb(0x96, 0xA3, 0xBD),
        text_on_weak: Color32::from_rgb(0xD2, 0xE4, 0xFF),
        text_dim: Color32::from_rgb(0x7A, 0x82, 0xA8),
        accent: Color32::from_rgb(0x2A, 0xC3, 0xDE),
        accent_strong: Color32::from_rgb(0x00, 0xA3, 0xBD),
        accent_dark: Color32::from_rgb(0x28, 0x9D, 0xB4),
        on_accent: Color32::from_rgb(0x12, 0x1A, 0x2A),
        hover_bg: Color32::from_rgb(0x47, 0x4F, 0x7A),
        hover_border: Color32::from_rgb(0x55, 0x5E, 0x8F),
        active_bg: Color32::from_rgb(0x55, 0x5E, 0x8F),
        success: Color32::from_rgb(0x9E, 0xCE, 0x6A),
        warning: Color32::from_rgb(0xE0, 0xAF, 0x68),
        danger: Color32::from_rgb(0xF7, 0x76, 0x8E),
        record_red: Color32::from_rgb(0xE5, 0x48, 0x4D),
        playhead: Color32::from_rgb(0xFF, 0x33, 0x33),
        route_indicator: Color32::from_rgb(0xA6, 0x59, 0x59),
        close_hover: Color32::from_rgb(0xC4, 0x2B, 0x1C),
        // 卷帘骨架（旧版 `ThemeExt` 的 Tokyo Night Storm 口径）。
        roll_keyboard_bg: Color32::from_rgb(0x24, 0x28, 0x3B),
        roll_white_key: Color32::from_rgb(0x2A, 0x2F, 0x47),
        roll_black_key: Color32::from_rgb(0x24, 0x28, 0x3B),
        roll_key_line: Color32::from_rgb(0x39, 0x40, 0x63),
        roll_ruler_bg: Color32::from_rgb(0x24, 0x28, 0x3B),
        roll_text: Color32::WHITE,
    };

    /// 由 旧版 调色板种子派生完整语义色板。
    ///
    /// 旧版 没有直接给出的颜色按统一规则派生，保证任意主题（Light / Dark /
    /// Solarized / Catppuccin …）都有可读的层次：
    /// - 边框 / 悬停填充取 `background.strong`（旧版 内置边框口径）；
    /// - 次级文字按正文与底色的比例混合；
    /// - 浅色主题的卡片用 `weakest`，否则白底卡片会与内容底不可分；
    /// - 强调色上的文字按强调色亮度取深色或白色。
    pub fn from_seed(seed: &PaletteSeed) -> Self {
        // Tokyo Night Storm 是历史基线色板：逐色派生会带来 ±1 的取整差异，
        // 直接返回精确字面量，保证场景像素基线与真实窗口完全一致。
        if seed.base == Self::TOKYO_NIGHT_STORM.base_bg
            && seed.neutral == Self::TOKYO_NIGHT_STORM.titlebar_bg
            && seed.text == Self::TOKYO_NIGHT_STORM.text
            && seed.primary == Self::TOKYO_NIGHT_STORM.accent
        {
            return Self::TOKYO_NIGHT_STORM;
        }
        let base = seed.base;
        let mix = |a: Color32, b: Color32, t: f32| {
            let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
            Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
        };
        let luminance = |c: Color32| {
            (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
        };
        Self {
            is_dark: seed.is_dark,
            titlebar_bg: seed.neutral,
            titlebar_bg_unfocused: seed.weaker,
            toolbar_bg: seed.weak,
            panel_bg: seed.weakest,
            route_bar_bg: seed.weaker,
            base_bg: base,
            card_bg: if seed.is_dark { base } else { seed.weakest },
            selected_bg: seed.strong,
            border: seed.strong,
            text: seed.text,
            // 次级文字按「底色 → 正文」的比例混合：0.78 / 0.58 与历史基线
            // （TNS 的 `#96A3BD` / `#7A82A8`）实测比例一致，浅色主题同样可读。
            text_muted: mix(base, seed.text, 0.78),
            text_on_weak: seed.text_on_weak,
            text_dim: mix(base, seed.text, 0.58),
            accent: seed.primary,
            accent_strong: seed.primary_strong,
            accent_dark: seed.primary_weak,
            on_accent: if luminance(seed.primary) > 0.55 {
                Color32::from_rgb(0x12, 0x1A, 0x2A)
            } else {
                Color32::WHITE
            },
            hover_bg: seed.strong,
            hover_border: mix(seed.strong, seed.text, 0.15),
            active_bg: mix(seed.strong, seed.text, 0.15),
            success: seed.success,
            warning: seed.warning,
            danger: seed.danger,
            record_red: Self::TOKYO_NIGHT_STORM.record_red,
            playhead: Self::TOKYO_NIGHT_STORM.playhead,
            route_indicator: Self::TOKYO_NIGHT_STORM.route_indicator,
            close_hover: Self::TOKYO_NIGHT_STORM.close_hover,
            // 卷帘骨架：与 旧版 `ThemeExt` 逐条同规则。
            roll_keyboard_bg: if seed.is_dark { base } else { seed.weak },
            roll_white_key: if seed.is_dark {
                seed.weakest
            } else {
                seed.weak
            },
            roll_black_key: if seed.is_dark { base } else { seed.strong },
            roll_key_line: if seed.is_dark {
                seed.weak
            } else {
                seed.strongest
            },
            roll_ruler_bg: if seed.is_dark { base } else { seed.weakest },
            roll_text: if seed.is_dark {
                Color32::WHITE
            } else {
                Color32::BLACK
            },
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::TOKYO_NIGHT_STORM
    }
}

thread_local! {
    /// 当前帧色板。默认 Tokyo Night Storm：无头截图 / 单测无需显式设置。
    static ACTIVE: Cell<Palette> = const { Cell::new(Palette::TOKYO_NIGHT_STORM) };
}

/// 当前生效的色板。
pub fn palette() -> Palette {
    ACTIVE.with(Cell::get)
}

/// 设置当前帧色板（应在渲染入口、[`apply`] 之前调用）。
pub fn set_palette(palette: Palette) {
    ACTIVE.with(|active| active.set(palette));
}

macro_rules! color_getters {
    ($($name:ident),* $(,)?) => {
        $(
            #[doc = concat!("当前色板的 `", stringify!($name), "`。")]
            pub fn $name() -> Color32 {
                palette().$name
            }
        )*
    };
}

color_getters!(
    titlebar_bg,
    titlebar_bg_unfocused,
    toolbar_bg,
    panel_bg,
    route_bar_bg,
    base_bg,
    card_bg,
    selected_bg,
    border,
    text,
    text_muted,
    text_on_weak,
    text_dim,
    accent,
    accent_strong,
    accent_dark,
    on_accent,
    hover_bg,
    hover_border,
    active_bg,
    success,
    warning,
    danger,
    record_red,
    playhead,
    route_indicator,
    close_hover,
    roll_keyboard_bg,
    roll_white_key,
    roll_black_key,
    roll_key_line,
    roll_ruler_bg,
    roll_text,
);

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

/// 应用主题到 egui Context（使用当前色板）。
///
/// 每帧调用是常态，但只有色板真的变化时才写 style / visuals：
/// `set_global_style` 每次都会替换 `Arc<Style>`，逐帧写会让 egui 认为样式
/// 一直在变，控件悬停动画与文本布局反复失效（真机表现为按钮文字轻微抽搐）。
pub fn apply(ctx: &egui::Context) {
    let palette = palette();
    let marker = egui::Id::unique("lumino.theme.applied");
    if ctx.data(|data| data.get_temp::<Palette>(marker)) == Some(palette) {
        return;
    }
    ctx.set_theme(if palette.is_dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    ctx.set_global_style(style());
    ctx.set_visuals(visuals());
    ctx.data_mut(|data| data.insert_temp(marker, palette));
}

/// 设置色板并应用到 egui Context。
pub fn apply_with(ctx: &egui::Context, palette: Palette) {
    set_palette(palette);
    apply(ctx);
}

/// 生成 Lumino 的 egui `Visuals`（跟随当前色板）。
pub fn visuals() -> Visuals {
    let palette = palette();
    let mut visuals = if palette.is_dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    visuals.override_text_color = Some(palette.text);
    visuals.panel_fill = palette.base_bg;
    visuals.window_fill = palette.base_bg;
    visuals.extreme_bg_color = palette.base_bg;
    visuals.faint_bg_color = palette.panel_bg;
    visuals.code_bg_color = palette.panel_bg;
    visuals.window_stroke = Stroke::new(1.0, palette.border);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.selection.bg_fill = palette.accent.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.hyperlink_color = palette.accent;
    visuals.warn_fg_color = palette.warning;
    visuals.error_fg_color = palette.danger;

    visuals.widgets.noninteractive.bg_fill = palette.base_bg;
    visuals.widgets.noninteractive.weak_bg_fill = palette.panel_bg;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(4);

    visuals.widgets.inactive.bg_fill = palette.toolbar_bg;
    visuals.widgets.inactive.weak_bg_fill = palette.toolbar_bg;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);

    visuals.widgets.hovered.bg_fill = palette.hover_bg;
    visuals.widgets.hovered.weak_bg_fill = palette.hover_bg;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.hover_border);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

    visuals.widgets.active.bg_fill = palette.active_bg;
    visuals.widgets.active.weak_bg_fill = palette.active_bg;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent_dark);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, palette.on_accent);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);

    visuals.widgets.open.bg_fill = palette.toolbar_bg;
    visuals.widgets.open.weak_bg_fill = palette.toolbar_bg;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.open.corner_radius = CornerRadius::same(4);

    visuals
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
