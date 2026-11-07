//! 外壳 band 的像素级几何回归：与 iced 基线同尺寸（1600×1000 @1x）逐列比较墨迹区间。
//!
//! 只比较「文字/图标出现的列区间」而不是逐像素颜色：两条渲染栈的字体栅格化
//! 必然不同（cosmic-text vs egui + YaHei），但**位置**必须一致——这正是
//! 视觉验收里「几何严格」的可执行版本。

use std::path::PathBuf;

use egui_kittest::Harness;
use image::RgbaImage;
use lumino_core::Tool;
use lumino_core::storage::config::AutoScrollMode;
use lumino_ui_egui::chrome::{self, ChromeModel, StatusbarModel, TitlebarModel, ToolbarModel};
use lumino_ui_egui::icons::IconCache;

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 1000;
/// 墨迹列区间的允许偏差（像素）。
///
/// 4px 预算用于吸收两条字体栈的字形宽度差异（cosmic-text vs egui 整形）；
/// 结构性位置（8px 内边距、组顺序、右锚点）本身是精确对齐的。
const TOLERANCE: i64 = 4;

const TITLEBAR_BG: [u8; 3] = [0x3F, 0x46, 0x6E];
const STATUSBAR_BG: [u8; 3] = [0x39, 0x40, 0x63];

fn model() -> ChromeModel {
    // 取值与 iced 基线捕获一致：就绪 + 0.0% / 0.0 MB / 60.0。
    ChromeModel {
        titlebar: Some(TitlebarModel {
            focused: true,
            menus: vec![
                "文件".to_owned(),
                "编辑".to_owned(),
                "视图".to_owned(),
                "帮助".to_owned(),
            ],
        }),
        toolbar: Some(ToolbarModel {
            left_inset: 248.0,
            bpm: "120.0".to_owned(),
            ppq: "PPQ 1920".to_owned(),
            position: "1.1".to_owned(),
            time: "00:00.00".to_owned(),
            snap: "1/4".to_owned(),
            tool: Tool::Pointer,
            draw_panel_open: false,
            playing: false,
            looping: false,
            recording: false,
            onion: false,
            auto_scroll: AutoScrollMode::ScrollingIndicator,
        }),
        statusbar: StatusbarModel {
            left: "就绪".to_owned(),
            left_accent: true,
            cpu_percent: Some(0.0),
            memory_mb: 0.0,
            fps: Some(60.0),
            right: String::new(),
        },
    }
}

fn render_bands() -> RgbaImage {
    let model = model();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .wgpu()
        .with_size(egui::vec2(WIDTH as f32, HEIGHT as f32))
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
        .expect("band 像素测试需要可用的 wgpu 渲染器")
}

fn iced_baseline() -> RgbaImage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("测试目录必须有工作区根")
        .join("tests/ui-baseline/iced/main_chrome-dpi100.png");
    image::open(&path)
        .expect("读取 iced 基线失败（先运行 capture_iced）")
        .to_rgba8()
}

/// 某一行带内与背景差异超过阈值的列区间 `(start, end)`（含端点）。
fn ink_runs(image: &RgbaImage, y0: u32, y1: u32, x0: u32, x1: u32, bg: [u8; 3]) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let mut start: Option<u32> = None;
    for x in x0..=x1 {
        let mut ink = false;
        for y in y0..=y1 {
            let pixel = image.get_pixel(x, y).0;
            let delta = (0..3)
                .map(|channel| i32::from(pixel[channel]) - i32::from(bg[channel]))
                .map(i32::abs)
                .sum::<i32>();
            if delta > 40 {
                ink = true;
                break;
            }
        }
        match (ink, start) {
            (true, None) => start = Some(x),
            (false, Some(from)) => {
                runs.push((from, x - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        runs.push((from, x1));
    }
    runs
}

/// 把相邻墨迹段合并成「文本块」：组内字距 1–3px，组间 ≥14px。
fn merge_runs(runs: &[(u32, u32)], max_gap: u32) -> Vec<(u32, u32)> {
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for &(start, end) in runs {
        match merged.last_mut() {
            Some(last) if start <= last.1 + max_gap => last.1 = end,
            _ => merged.push((start, end)),
        }
    }
    merged
}

fn assert_runs_close(actual: &[(u32, u32)], expected: &[(u32, u32)], label: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label} 墨迹区间数量不一致：\n实际 {actual:?}\n期望 {expected:?}"
    );
    for (index, (got, want)) in actual.iter().zip(expected).enumerate() {
        let start_delta = i64::from(got.0) - i64::from(want.0);
        let end_delta = i64::from(got.1) - i64::from(want.1);
        assert!(
            start_delta.abs() <= TOLERANCE && end_delta.abs() <= TOLERANCE,
            "{label} 第 {index} 段偏移超限：实际 {got:?}，期望 {want:?}（容差 {TOLERANCE}px）"
        );
    }
}

#[test]
fn bands_match_iced_geometry() {
    let bands = render_bands();
    let baseline = iced_baseline();
    // 失败时便于人工核对的调试图（workspace/target）。
    let debug_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("测试目录必须有工作区根")
        .join("target/bands-debug.png");
    bands.save(&debug_path).expect("写入 band 调试图失败");

    // 底色必须逐字节一致：band 是不透明覆盖层，色差会造成整条可见接缝。
    let titlebar_pixel = bands.get_pixel(800, 15).0;
    assert_eq!(
        &titlebar_pixel[..3],
        &TITLEBAR_BG,
        "标题栏底色与 iced `background.neutral` 不一致"
    );
    let statusbar_pixel = bands.get_pixel(800, 990).0;
    assert_eq!(
        &statusbar_pixel[..3],
        &STATUSBAR_BG,
        "状态栏底色与 iced `background.weak` 不一致"
    );

    // 标题栏：logo + 四个菜单标签。
    let bands_titlebar = ink_runs(&bands, 2, 28, 0, 340, TITLEBAR_BG);
    let iced_titlebar = ink_runs(&baseline, 2, 28, 0, 340, TITLEBAR_BG);
    assert_runs_close(&bands_titlebar, &iced_titlebar, "标题栏");

    // 状态栏左侧描述文本。
    let bands_left = ink_runs(&bands, 982, 997, 0, 120, STATUSBAR_BG);
    let iced_left = ink_runs(&baseline, 982, 997, 0, 120, STATUSBAR_BG);
    assert_runs_close(&bands_left, &iced_left, "状态栏左侧");

    // 状态栏右侧 CPU / MEM / FPS 指标组：字形切分不同，按文本块比较。
    let bands_right = merge_runs(&ink_runs(&bands, 982, 997, 1330, 1599, STATUSBAR_BG), 6);
    let iced_right = merge_runs(&ink_runs(&baseline, 982, 997, 1330, 1599, STATUSBAR_BG), 6);
    assert_runs_close(&bands_right, &iced_right, "状态栏指标");

    // 工具栏 band：必须从左侧栏右缘 248 开始（band 内为 PANEL_BG，band 外不绘制）。
    assert_ne!(
        &bands.get_pixel(247, 60).0[..3],
        &[0x2A, 0x2F, 0x47],
        "工具栏 band 不得覆盖左侧栏区域"
    );
    assert_eq!(
        &bands.get_pixel(248, 60).0[..3],
        &[0x2A, 0x2F, 0x47],
        "工具栏 band 左边界应为左侧栏宽度 248"
    );
    let bands_toolbar = ink_runs(&bands, 38, 88, 248, 1599, [0x2A, 0x2F, 0x47]);
    let iced_toolbar = ink_runs(&baseline, 38, 88, 248, 1599, [0x2A, 0x2F, 0x47]);
    assert_runs_close(&bands_toolbar, &iced_toolbar, "工具栏");
}
