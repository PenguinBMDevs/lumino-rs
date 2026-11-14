//! 卷帘骨架的「标尺让位」回归：`draw_ruler = false` 时只跳过标尺，
//! 键盘列与两条滚动条必须照常绘制。
//!
//! 背景：键盘列只由 egui 绘制（gfx 网格着色器 discard 键盘列），
//! 标尺在拍号分段时让位给 gfx。两者若共用一个开关，加载带中途变拍的
//! MIDI 后键盘会跟着标尺一起消失。
//!
//! 只用一个测试串行渲染两帧：同一进程并发创建多个 wgpu harness 在部分
//! 驱动上会触发访问冲突（0xc0000005）。

use egui_kittest::Harness;
use image::RgbaImage;
use lumino_ui_egui::chrome::piano_roll::{self, PianoRollView};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn view(draw_ruler: bool) -> PianoRollView {
    PianoRollView {
        left_inset: 0.0,
        top_inset: 0.0,
        right_inset: 0.0,
        bottom_inset: 0.0,
        draw_ruler,
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
fn ruler_gate_keeps_keyboard_and_scrollbars() {
    // draw_ruler = true：键盘列 / 标尺 / 两条滚动条全部绘制。
    let with_ruler = render(view(true));
    assert!(alpha(&with_ruler, 10, 100) > 0, "键盘列必须绘制");
    assert!(
        alpha(&with_ruler, 200, 10) > 0,
        "draw_ruler=true 时标尺必须绘制"
    );
    assert!(alpha(&with_ruler, 790, 100) > 0, "垂直滚动条必须绘制");
    assert!(alpha(&with_ruler, 100, 590) > 0, "水平滚动条必须绘制");
    assert_eq!(alpha(&with_ruler, 400, 300), 0, "中央网格区域必须保持透明");

    // draw_ruler = false：只跳过标尺，键盘 / 滚动条不得跟着消失。
    let without_ruler = render(view(false));
    assert_eq!(
        alpha(&without_ruler, 200, 10),
        0,
        "draw_ruler=false 时标尺必须让位"
    );
    assert!(
        alpha(&without_ruler, 10, 100) > 0,
        "键盘列不得随标尺一起消失"
    );
    assert!(alpha(&without_ruler, 10, 10) > 0, "键盘左上角块必须保留");
    assert!(
        alpha(&without_ruler, 790, 100) > 0,
        "垂直滚动条不得随标尺一起消失"
    );
    assert!(
        alpha(&without_ruler, 100, 590) > 0,
        "水平滚动条不得随标尺一起消失"
    );
    assert_eq!(
        alpha(&without_ruler, 400, 300),
        0,
        "中央网格区域必须保持透明"
    );
}
