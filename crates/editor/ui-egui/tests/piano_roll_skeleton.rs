//! 卷帘骨架像素回归：键盘列 / 标尺 / 两条滚动条始终由 egui 绘制。
//!
//! 背景：键盘列只由 egui 绘制（gfx 网格着色器 discard 键盘列），横向标尺的
//! 小节号也只能由 egui 绘制（gfx `RulerRenderer` 只有刻度线）。拍号中途
//! 变化时曾把标尺让位给 gfx，结果小节号消失；现在 egui 标尺自己处理拍号
//! 分段，骨架三块必须同时存在。
//!
//! 只用一个测试串行渲染：同一进程并发创建多个 wgpu harness 在部分驱动上
//! 会触发访问冲突（0xc0000005）。

use egui_kittest::Harness;
use image::RgbaImage;
use lumino_ui_egui::chrome::piano_roll::{self, PianoRollView};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn view() -> PianoRollView {
    PianoRollView {
        left_inset: 0.0,
        top_inset: 0.0,
        right_inset: 0.0,
        bottom_inset: 0.0,
        // 中途变拍：4/4 @0，3/4 @7680。标尺必须继续画，且小节号连续。
        time_signatures: vec![(0, 4, 4), (7680, 3, 4)],
        ..Default::default()
    }
}

fn render(view: PianoRollView) -> RgbaImage {
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
        .build_ui(move |ui| piano_roll::draw_chrome(ui, &view));
    harness.run();
    harness
        .render()
        .expect("卷帘骨架像素测试需要可用的 wgpu 渲染器")
}

fn alpha(image: &RgbaImage, x: u32, y: u32) -> u8 {
    image.get_pixel(x, y).0[3]
}

#[test]
fn skeleton_keeps_keyboard_ruler_and_scrollbars() {
    let image = render(view());

    // 键盘列（x < 120）、标尺（y < 24, x >= 120）、两条滚动条（右 20 / 下 20）。
    assert!(alpha(&image, 10, 100) > 0, "键盘列必须绘制");
    assert!(
        alpha(&image, 200, 10) > 0,
        "中途变拍时标尺底盘 / 小节号必须绘制"
    );
    assert!(alpha(&image, 790, 100) > 0, "垂直滚动条必须绘制");
    assert!(alpha(&image, 100, 590) > 0, "水平滚动条必须绘制");
    // 标尺底盘与网格同色，真正可见的是小节号文本：必须存在高亮像素。
    let has_label = (120..WIDTH - 20).any(|x| {
        (0..24).any(|y| {
            let pixel = image.get_pixel(x, y).0;
            pixel[0] > 100 && pixel[1] > 100 && pixel[2] > 100
        })
    });
    assert!(has_label, "中途变拍时标尺必须画出小节号文本");
    assert_eq!(alpha(&image, 400, 300), 0, "中央网格区域必须保持透明");
}
