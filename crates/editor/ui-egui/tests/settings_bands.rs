//! 设置对话框整窗 band 的像素回归：深色 / 浅色 / 英文三种场景共用同一份
//! 渲染代码，几何与 iced 基线一致。

mod common;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use egui_kittest::Harness;
use lumino_core::storage::config::{EraserBehavior, TrackAddBehavior};
use lumino_extras::i18n::{
    Language, eraser_behavior_name, settings_translations, track_add_behavior_name,
};
use lumino_ui_egui::chrome::{
    self, ChromeModel, DialogModel, SettingsLabels, SettingsPalette, SettingsSnapshot,
};
use lumino_ui_egui::icons::IconCache;

use common::{assert_runs_close, assert_runs_close_with, ink_row_runs, ink_runs};

/// `egui_kittest` 的 wgpu 渲染器不是线程安全的：同一测试二进制内的像素测试
/// 必须串行执行（并行时偶发 `STATUS_ACCESS_VIOLATION`）。
fn gpu_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 生产设置面板文案（与 `Root::egui_dialog_model` 同源，中/英文都走这里）。
fn settings_labels(language: Language) -> SettingsLabels {
    let t = settings_translations(language);
    SettingsLabels {
        menu: vec![
            t.general.to_owned(),
            t.audio.to_owned(),
            t.ui.to_owned(),
            t.shortcuts.to_owned(),
            t.onion_skin.to_owned(),
            t.palette.to_owned(),
            t.editing.to_owned(),
            t.cloud_manage.to_owned(),
            t.compatibility.to_owned(),
            t.about.to_owned(),
        ],
        title: t.general_title.to_owned(),
        eraser_label: t.eraser_behavior.to_owned(),
        eraser_options: vec![
            eraser_behavior_name(EraserBehavior::Default, language).to_owned(),
            eraser_behavior_name(EraserBehavior::DirectSelect, language).to_owned(),
        ],
        track_add_label: t.track_add_behavior.to_owned(),
        track_add_options: vec![
            track_add_behavior_name(TrackAddBehavior::AutoSwitch, language).to_owned(),
            track_add_behavior_name(TrackAddBehavior::StayCurrent, language).to_owned(),
        ],
        log_section: t.log_retention_section.to_owned(),
        log_label: t.log_retention_count.to_owned(),
        log_placeholder: "10".to_owned(),
        confirm: t.confirm.to_owned(),
    }
}

/// 设置对话框「常规」页整窗：`show_bands` 的输出必须与 iced 基线同几何。
#[test]
fn settings_dialog_bands_match_iced_geometry() {
    let _guard = gpu_lock();
    const W: u32 = 1200;
    const H: u32 = 800;
    let model = ChromeModel {
        dialog: Some(DialogModel::Settings(Box::new(SettingsSnapshot {
            palette: SettingsPalette::DARK,
            labels: settings_labels(Language::ZhCn),
            selected: 0,
            eraser: "默认 (Shift+拖动框选)".to_owned(),
            track_add: "自动跳转到新音轨".to_owned(),
            log_retention: "10".to_owned(),
            page: lumino_ui_egui::chrome::SettingsPageModel::General,
        }))),
        ..ChromeModel::default()
    };
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(W as f32, H as f32))
        .with_pixels_per_point(1.0)
        .with_render_cursor(false)
        .with_accessibility_check(false)
        .with_context_setup(|ctx| {
            lumino_ui_egui::theme::apply(ctx);
            lumino_ui_egui::fonts::install(ctx);
        })
        .build_ui(move |ui| chrome::show_bands(ui, &model, &mut icons));
    harness.run();
    let bands = harness.render().expect("设置对话框需要可用的渲染器");

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("测试目录必须有工作区根")
        .join("tests/ui-baseline/iced/settings_general-dpi100.png");
    let baseline = image::open(&path)
        .expect("读取 iced 基线失败（先运行 capture_iced）")
        .to_rgba8();

    // 标题栏 / 内容 / 菜单底色（与场景一致）。
    assert_eq!(&bands.get_pixel(10, 15).0[..3], &[0x3F, 0x46, 0x6E]);
    assert_eq!(&bands.get_pixel(10, 40).0[..3], &[0x24, 0x28, 0x3B]);
    assert_eq!(&bands.get_pixel(45, 300).0[..3], &[0x39, 0x40, 0x63]);

    // 整窗差异率：与场景同级（<5%）。
    let mut different = 0_u64;
    for y in 0..H {
        for x in 0..W {
            let a = bands.get_pixel(x, y).0;
            let b = baseline.get_pixel(x, y).0;
            let delta = (0..3)
                .map(|c| i32::from(a[c]) - i32::from(b[c]))
                .map(i32::abs)
                .max()
                .unwrap_or(0);
            if delta > 2 {
                different += 1;
            }
        }
    }
    let ratio = different as f64 / f64::from(W * H);
    assert!(ratio < 0.05, "设置对话框整窗差异率过高：{:.4}", ratio);
}

/// 浅色主题：调色板由生产主题注入后，设置对话框的表面颜色必须与 iced 基线一致。
#[test]
fn settings_dialog_light_theme_uses_injected_palette() {
    let _guard = gpu_lock();
    const W: u32 = 1200;
    const H: u32 = 800;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("测试目录必须有工作区根")
        .join("tests/ui-baseline/iced/settings_general_light-dpi100.png");
    let baseline = image::open(&path)
        .expect("读取浅色 iced 基线失败（先运行 capture_iced）")
        .to_rgba8();

    let sample = |x: u32, y: u32| {
        let p = baseline.get_pixel(x, y).0;
        egui::Color32::from_rgb(p[0], p[1], p[2])
    };
    // 表面颜色直接取自浅色 iced 基线（标题栏/内容/容器/菜单/卡片）。
    let titlebar = sample(10, 15);
    let background = sample(10, 40);
    let panel = sample(20, 300);
    let toolbar = sample(45, 300);
    let card = sample(1000, 300);
    assert_ne!(
        background,
        SettingsPalette::DARK.background,
        "浅色基线必须是浅色主题"
    );

    let model = ChromeModel {
        dialog: Some(DialogModel::Settings(Box::new(SettingsSnapshot {
            palette: SettingsPalette {
                titlebar,
                background,
                panel,
                toolbar,
                card,
                ..SettingsPalette::DARK
            },
            labels: settings_labels(Language::ZhCn),
            selected: 0,
            eraser: "默认 (Shift+拖动框选)".to_owned(),
            track_add: "自动跳转到新音轨".to_owned(),
            log_retention: "10".to_owned(),
            page: lumino_ui_egui::chrome::SettingsPageModel::General,
        }))),
        ..ChromeModel::default()
    };
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(W as f32, H as f32))
        .with_pixels_per_point(1.0)
        .with_render_cursor(false)
        .with_accessibility_check(false)
        .with_context_setup(|ctx| {
            lumino_ui_egui::theme::apply(ctx);
            lumino_ui_egui::fonts::install(ctx);
        })
        .build_ui(move |ui| chrome::show_bands(ui, &model, &mut icons));
    harness.run();
    let bands = harness.render().expect("浅色设置对话框需要可用的渲染器");

    // 表面颜色逐点一致：说明调色板注入链路正确。
    for (label, x, y, expected) in [
        ("标题栏", 10, 15, titlebar),
        ("内容", 10, 40, background),
        ("容器", 20, 300, panel),
        ("菜单", 45, 300, toolbar),
        ("卡片", 1000, 300, card),
    ] {
        assert_eq!(
            &bands.get_pixel(x, y).0[..3],
            &[expected.r(), expected.g(), expected.b()],
            "{label}底色与 iced 浅色基线不一致"
        );
    }

    // 菜单文字几何与浅色基线一致（颜色差异不影响墨迹列区间）。
    let bands_menu = ink_runs(
        &bands,
        84,
        104,
        60,
        230,
        [toolbar.r(), toolbar.g(), toolbar.b()],
    );
    let iced_menu = ink_runs(
        &baseline,
        84,
        104,
        60,
        230,
        [toolbar.r(), toolbar.g(), toolbar.b()],
    );
    assert_runs_close(&bands_menu, &iced_menu, "浅色设置菜单首行");
}

/// 英文界面：文案全部来自生产 `settings_translations(EnUs)`，几何与英文 iced 基线一致。
#[test]
fn settings_dialog_english_labels_match_iced_geometry() {
    let _guard = gpu_lock();
    const W: u32 = 1200;
    const H: u32 = 800;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("测试目录必须有工作区根")
        .join("tests/ui-baseline/iced/settings_general_en-dpi100.png");
    let baseline = image::open(&path)
        .expect("读取英文 iced 基线失败（先运行 capture_iced）")
        .to_rgba8();

    let labels = settings_labels(Language::EnUs);
    assert_eq!(labels.title, "General");
    assert_eq!(labels.confirm, "Confirm");
    assert_eq!(labels.menu.len(), 10);
    assert_eq!(labels.menu[0], "General");

    let model = ChromeModel {
        dialog: Some(DialogModel::Settings(Box::new(SettingsSnapshot {
            palette: SettingsPalette::DARK,
            labels,
            selected: 0,
            eraser: eraser_behavior_name(EraserBehavior::Default, Language::EnUs).to_owned(),
            track_add: track_add_behavior_name(TrackAddBehavior::AutoSwitch, Language::EnUs)
                .to_owned(),
            log_retention: "10".to_owned(),
            page: lumino_ui_egui::chrome::SettingsPageModel::General,
        }))),
        ..ChromeModel::default()
    };
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(W as f32, H as f32))
        .with_pixels_per_point(1.0)
        .with_render_cursor(false)
        .with_accessibility_check(false)
        .with_context_setup(|ctx| {
            lumino_ui_egui::theme::apply(ctx);
            lumino_ui_egui::fonts::install(ctx);
        })
        .build_ui(move |ui| chrome::show_bands(ui, &model, &mut icons));
    harness.run();
    let bands = harness.render().expect("英文设置对话框需要可用的渲染器");

    // 表面底色与深色基线一致（语言不影响主题）。
    assert_eq!(&bands.get_pixel(10, 15).0[..3], &[0x3F, 0x46, 0x6E]);
    assert_eq!(&bands.get_pixel(10, 40).0[..3], &[0x24, 0x28, 0x3B]);

    // 菜单首行（"General"）：只比较图标右侧的文字区，避免图标栅格差异。
    let bands_menu = ink_runs(&bands, 50, 74, 90, 234, [0x39, 0x40, 0x63]);
    let iced_menu = ink_runs(&baseline, 50, 74, 90, 234, [0x39, 0x40, 0x63]);
    assert_runs_close_with(&bands_menu, &iced_menu, 6, "英文设置菜单首行");

    // 橡皮擦行为行（"Eraser Behavior:" + 下拉文本）。
    let bands_row = ink_runs(&bands, 126, 146, 256, 700, [0x24, 0x28, 0x3B]);
    let iced_row = ink_runs(&baseline, 126, 146, 256, 700, [0x24, 0x28, 0x3B]);
    assert_runs_close_with(&bands_row, &iced_row, 6, "英文设置橡皮擦行");

    // 日志行标签（"Log file retention count:"）。
    let bands_log = ink_runs(&bands, 282, 300, 256, 700, [0x24, 0x28, 0x3B]);
    let iced_log = ink_runs(&baseline, 282, 300, 256, 700, [0x24, 0x28, 0x3B]);
    assert_runs_close_with(&bands_log, &iced_log, 8, "英文设置日志行");
}

/// 小窗口（720×540 逻辑点）：10 个菜单项放不下，但菜单必须能由 egui 自己滚，
/// 不能因为「放不下」把整窗交回 iced（那会削减接管范围）。
#[test]
fn settings_menu_scrolls_to_last_item_on_small_window() {
    let _guard = gpu_lock();
    const W: u32 = 720;
    const H: u32 = 540;
    /// 菜单底色（`SettingsPalette::DARK.toolbar`）。
    const MENU_BG: [u8; 3] = [0x39, 0x40, 0x63];

    let model = ChromeModel {
        dialog: Some(DialogModel::Settings(Box::new(SettingsSnapshot {
            palette: SettingsPalette::DARK,
            labels: settings_labels(Language::ZhCn),
            selected: 0,
            eraser: eraser_behavior_name(EraserBehavior::Default, Language::ZhCn).to_owned(),
            track_add: track_add_behavior_name(TrackAddBehavior::AutoSwitch, Language::ZhCn)
                .to_owned(),
            log_retention: "10".to_owned(),
            page: lumino_ui_egui::chrome::SettingsPageModel::General,
        }))),
        ..ChromeModel::default()
    };
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(W as f32, H as f32))
        .with_pixels_per_point(1.0)
        .with_render_cursor(false)
        .with_accessibility_check(false)
        .with_context_setup(|ctx| {
            lumino_ui_egui::theme::apply(ctx);
            lumino_ui_egui::fonts::install(ctx);
        })
        .build_ui(move |ui| chrome::show_bands(ui, &model, &mut icons));
    harness.run();

    // 菜单可视区 y = 66..454（30 标题栏 + 16 外边距 + 20 内边距起，扣确认按钮区）。
    let before = harness.render().expect("设置对话框需要可用的渲染器");
    assert!(
        !ink_row_runs(&before, 94, 220, 84, 102, MENU_BG).is_empty(),
        "滚动前首项「常规」应在菜单顶部"
    );
    assert!(
        ink_row_runs(&before, 94, 220, 420, 446, MENU_BG).is_empty(),
        "滚动前第 10 项「关于」在可视区之外（不能提前出现）"
    );

    // 滚轮只对指针悬停的 `ScrollArea` 生效：先把指针放到菜单上。
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(120.0, 300.0)));
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, -20.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(60);

    let after = harness.render().expect("设置对话框需要可用的渲染器");
    assert!(
        ink_row_runs(&after, 94, 220, 84, 102, MENU_BG).is_empty(),
        "滚到底后首项「常规」应移出可视区"
    );
    assert!(
        !ink_row_runs(&after, 94, 220, 420, 440, MENU_BG).is_empty(),
        "滚轮滚到底后第 10 项「关于」必须可见（菜单项一个都不能少）"
    );
}
