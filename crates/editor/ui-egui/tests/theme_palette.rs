//! 主题色板回归：egui 外壳必须跟随 iced 当前主题，而不是固定深色。
//!
//! 1. Tokyo Night Storm 种子必须还原历史基线字面量（像素基线零变化）；
//! 2. `Light` 种子必须派生出浅色外壳（默认配置就是 Light）；
//! 3. 实际渲染：浅色色板下 band 底色为浅色，深色色板下为深色。

use egui::Color32;
use egui_kittest::Harness;
use image::RgbaImage;
use lumino_ui_egui::chrome::{self, ChromeModel, StatusbarModel, TitlebarModel};
use lumino_ui_egui::icons::IconCache;
use lumino_ui_egui::theme::{Palette, PaletteSeed};

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 120.0;

/// 测试期间切换色板，结束（含 panic）后还原，避免同线程后续用例被污染。
struct PaletteGuard {
    previous: Palette,
}

impl PaletteGuard {
    fn new(palette: Palette) -> Self {
        let previous = lumino_ui_egui::theme::palette();
        lumino_ui_egui::theme::set_palette(palette);
        Self { previous }
    }
}

impl Drop for PaletteGuard {
    fn drop(&mut self) {
        lumino_ui_egui::theme::set_palette(self.previous);
    }
}

fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

/// iced `Theme::Light.extended_palette()` 的精确值。
fn light_seed() -> PaletteSeed {
    PaletteSeed {
        is_dark: false,
        base: rgb(0xFFFFFF),
        weakest: rgb(0xF5F5F5),
        weaker: rgb(0xE8E8E8),
        weak: rgb(0xDEDEDE),
        neutral: rgb(0xD6D6D6),
        strong: rgb(0xCECECE),
        text: rgb(0x000000),
        text_on_weak: rgb(0x000000),
        primary: rgb(0x5865F2),
        primary_strong: rgb(0x6B75FF),
        primary_weak: rgb(0xB4B8F7),
        success: rgb(0x12664F),
        warning: rgb(0xB77E33),
        danger: rgb(0xC3423F),
    }
}

/// iced `Theme::TokyoNightStorm.extended_palette()` 的精确值。
fn tokyo_night_storm_seed() -> PaletteSeed {
    PaletteSeed {
        is_dark: true,
        base: rgb(0x24283B),
        weakest: rgb(0x2A2F47),
        weaker: rgb(0x333957),
        weak: rgb(0x394063),
        neutral: rgb(0x3F466E),
        strong: rgb(0x444D79),
        text: rgb(0xB6C4FA),
        text_on_weak: rgb(0xD2E4FF),
        primary: rgb(0x2AC3DE),
        primary_strong: rgb(0x00A3BD),
        primary_weak: rgb(0x289DB4),
        success: rgb(0x9ECE6A),
        warning: rgb(0xE0AF68),
        danger: rgb(0xF7768E),
    }
}

#[test]
fn tokyo_night_storm_seed_keeps_historical_palette() {
    assert_eq!(
        Palette::from_seed(&tokyo_night_storm_seed()),
        Palette::TOKYO_NIGHT_STORM,
        "Tokyo Night Storm 必须精确还原历史基线（场景像素基线依赖它）"
    );
}

#[test]
fn light_seed_derives_light_chrome() {
    let palette = Palette::from_seed(&light_seed());
    assert!(!palette.is_dark, "Light 主题必须派生浅色外壳");
    assert_eq!(
        palette.titlebar_bg,
        rgb(0xD6D6D6),
        "标题栏 = background.neutral"
    );
    assert_eq!(
        palette.toolbar_bg,
        rgb(0xDEDEDE),
        "工具栏 = background.weak"
    );
    assert_eq!(palette.panel_bg, rgb(0xF5F5F5), "面板 = background.weakest");
    assert_eq!(
        palette.card_bg,
        rgb(0xF5F5F5),
        "浅色卡片必须用 weakest，否则白底卡片与内容不可分"
    );
    assert_eq!(palette.text, Color32::BLACK);
    assert_eq!(palette.on_accent, Color32::WHITE, "深色强调色上必须用白字");

    // 次级文字应介于正文与底色之间（可读且明显弱化）。
    let luminance = |c: Color32| {
        (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
    };
    let muted = luminance(palette.text_muted);
    assert!(
        muted < 0.4,
        "浅色主题的次级文字必须足够深（可读），实际亮度 {muted}"
    );
    assert!(luminance(palette.text_dim) > muted, "dim 应比 muted 更浅");

    // 与历史深色色板明确不同：不能出现「深色外壳 + 浅色内容」。
    assert_ne!(palette, Palette::TOKYO_NIGHT_STORM);
    assert!(luminance(palette.titlebar_bg) > 0.7);
}

#[test]
fn dark_seed_stays_dark() {
    let mut seed = light_seed();
    seed.is_dark = true;
    seed.neutral = rgb(0x3F466E);
    seed.text = rgb(0xB6C4FA);
    seed.base = rgb(0x24283B);
    let palette = Palette::from_seed(&seed);
    assert!(palette.is_dark);
    assert_eq!(palette.titlebar_bg, rgb(0x3F466E));
    assert_eq!(palette.card_bg, rgb(0x24283B), "深色卡片与内容底同色");
}

fn model() -> ChromeModel {
    ChromeModel {
        titlebar: Some(TitlebarModel {
            focused: true,
            menus: vec!["文件".to_owned(), "编辑".to_owned()],
            menu: None,
            menu_only: false,
        }),
        statusbar: StatusbarModel {
            left: "就绪".to_owned(),
            left_accent: true,
            cpu_percent: None,
            memory_mb: 0.0,
            fps: None,
            right: String::new(),
        },
        ..ChromeModel::default()
    }
}

fn render_bands(palette: Palette) -> RgbaImage {
    let _guard = PaletteGuard::new(palette);
    let model = model();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .wgpu()
        .with_size(egui::vec2(WIDTH, HEIGHT))
        .with_pixels_per_point(1.0)
        .with_render_cursor(false)
        .with_accessibility_check(false)
        .with_context_setup(|ctx| {
            lumino_ui_egui::theme::apply(ctx);
            lumino_ui_egui::fonts::install(ctx);
        })
        .build_ui(move |ui| chrome::show_bands(ui, &model, &mut icons));
    harness.run();
    harness
        .render()
        .expect("主题像素测试需要可用的 wgpu 渲染器")
}

#[test]
fn rendered_bands_follow_the_palette() {
    let light = render_bands(Palette::from_seed(&light_seed()));
    // 标题栏（y=15）与状态栏（y=110 附近）都必须是浅色底。
    let titlebar = light.get_pixel(400, 15).0;
    assert_eq!(
        &titlebar[..3],
        &[0xD6, 0xD6, 0xD6],
        "浅色主题下标题栏 band 必须为 background.neutral"
    );
    let statusbar = light.get_pixel(400, 110).0;
    assert_eq!(
        &statusbar[..3],
        &[0xDE, 0xDE, 0xDE],
        "浅色主题下状态栏 band 必须为 background.weak"
    );

    let dark = render_bands(Palette::TOKYO_NIGHT_STORM);
    let titlebar = dark.get_pixel(400, 15).0;
    assert_eq!(
        &titlebar[..3],
        &[0x3F, 0x46, 0x6E],
        "默认色板仍是历史基线（像素基线零变化）"
    );
}
