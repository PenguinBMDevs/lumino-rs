//! 未保存更改确认对话框整窗 band 的像素回归：`show_bands` 渲染的对话框
//! 必须与 legacy 基线（560×260 @1x）逐段一致。

mod common;

use std::path::PathBuf;

use egui_kittest::Harness;
use image::RgbaImage;
use lumino_ui_egui::chrome::{self, ChromeModel, DialogModel, DialogPalette, SaveConfirmModel};
use lumino_ui_egui::icons::IconCache;

use common::{assert_runs_close, assert_runs_close_with, ink_row_runs, ink_runs};

const WIDTH: u32 = 560;
const HEIGHT: u32 = 260;

fn model() -> ChromeModel {
    ChromeModel {
        dialog: Some(DialogModel::SaveConfirm(SaveConfirmModel {
            titlebar: true,
            palette: DialogPalette::DARK,
            message: "是否保留未保存的更改?".to_owned(),
            save: "保存".to_owned(),
            close: "关闭".to_owned(),
            cancel: "取消".to_owned(),
        })),
        ..ChromeModel::default()
    }
}

fn render() -> RgbaImage {
    let model = model();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
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
        .expect("对话框 band 像素测试需要可用的渲染器")
}

fn baseline() -> RgbaImage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("测试目录必须有工作区根")
        .join("tests/ui-baseline/legacy/save_confirm_dialog-dpi100.png");
    image::open(&path)
        .expect("读取 legacy 基线失败（先运行 capture_egui）")
        .to_rgba8()
}

#[test]
fn save_confirm_bands_match_legacy_geometry() {
    let bands = render();
    let baseline = baseline();
    let debug_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("测试目录必须有工作区根")
        .join("target/dialog-bands-debug.png");
    bands.save(&debug_path).expect("写入对话框调试图失败");

    // 自制标题栏 30px + 内容底色。
    assert_eq!(
        &bands.get_pixel(10, 15).0[..3],
        &[0x3F, 0x46, 0x6E],
        "标题栏底色应为 `background.neutral`"
    );
    assert_eq!(
        &bands.get_pixel(10, 40).0[..3],
        &[0x24, 0x28, 0x3B],
        "内容底色应为 `background.base`"
    );
    assert_eq!(
        bands.get_pixel(10, 250).0[3],
        255,
        "整窗必须不透明（对话框是独立窗口，下层没有 legacy 画面）"
    );

    // 按钮几何（legacy 基线 y=162）：135..224 / 235..324 / 335..424。
    assert_eq!(
        &bands.get_pixel(180, 162).0[..3],
        &[0x2A, 0xC3, 0xDE],
        "主按钮底色应为 `primary.base`"
    );
    assert_eq!(
        &bands.get_pixel(280, 162).0[..3],
        &[0x39, 0x40, 0x63],
        "次按钮底色应为 `background.weak`"
    );
    assert_eq!(
        &bands.get_pixel(380, 162).0[..3],
        &[0x39, 0x40, 0x63],
        "取消按钮底色应为 `background.weak`"
    );
    assert_eq!(
        &bands.get_pixel(130, 162).0[..3],
        &[0x24, 0x28, 0x3B],
        "按钮左侧应保持内容底色"
    );

    // 提示语：两套字形栈的整形宽度不同（<2px），墨迹按整段比较、容差 6px。
    let merge = |runs: Vec<(u32, u32)>| -> Vec<(u32, u32)> {
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for (start, end) in runs {
            match merged.last_mut() {
                Some(last) if start <= last.1 + 8 => last.1 = end,
                _ => merged.push((start, end)),
            }
        }
        merged
    };
    let bands_text = merge(ink_runs(&bands, 110, 135, 100, 460, [0x24, 0x28, 0x3B]));
    let legacy = merge(ink_runs(&baseline, 110, 135, 100, 460, [0x24, 0x28, 0x3B]));
    assert_runs_close_with(&bands_text, &legacy, 6, "保存确认提示语");

    // 按钮文字墨迹行。
    let bands_button_text = ink_row_runs(&bands, 135, 424, 150, 200, [0x2A, 0xC3, 0xDE]);
    let legacy_button_text = ink_row_runs(&baseline, 135, 424, 150, 200, [0x2A, 0xC3, 0xDE]);
    assert_runs_close(&bands_button_text, &legacy_button_text, "主按钮文字");

    // 整体差异率：字体栅格化差异造成的残差应保持在场景同级（<5%）。
    let mut different = 0_u64;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
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
    let ratio = different as f64 / f64::from(WIDTH * HEIGHT);
    assert!(ratio < 0.05, "保存确认对话框整窗差异率过高：{:.4}", ratio);
}
