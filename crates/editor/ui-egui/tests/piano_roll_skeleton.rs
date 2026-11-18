//! 卷帘骨架像素回归：键盘列 / 标尺 / 两条滚动条 / 演奏指示线始终由 egui 绘制。
//!
//! 背景：键盘列只由 egui 绘制（gfx 网格着色器 discard 键盘列），横向标尺的
//! 小节号也只能由 egui 绘制（gfx `RulerRenderer` 只有刻度线）。拍号中途
//! 变化时曾把标尺让位给 gfx，结果小节号消失；现在 egui 标尺自己处理拍号
//! 分段，骨架三块必须同时存在。
//!
//! 演奏指示线曾是 egui 迁移的回归点：只剩标尺行内一段，网格区域的整条线
//! 丢失（观感为被卷帘内容盖住）。它必须是骨架的最后一层并纵贯网格整高。
//!
//! 只用一个测试串行渲染：同一进程并发创建多个 wgpu harness 在部分驱动上
//! 会触发访问冲突（0xc0000005）。

use egui_kittest::Harness;
use image::RgbaImage;
use lumino_ui_egui::chrome::piano_roll::{self, PianoRollView};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
/// 指示线相对网格左缘的偏移（测试值，避开键盘列与右缘）。
const PLAYHEAD_OFFSET: u32 = 100;
/// 键盘列宽度。
const KEYBOARD_W: u32 = 120;
/// 滚动条厚度。
const SCROLLBAR: u32 = 20;

fn view() -> PianoRollView {
    PianoRollView {
        left_inset: 0.0,
        top_inset: 0.0,
        right_inset: 0.0,
        bottom_inset: 0.0,
        // 中途变拍：4/4 @0，3/4 @7680。标尺必须继续画，且小节号连续。
        time_signatures: vec![(0, 4, 4), (7680, 3, 4)],
        playhead_offset: PLAYHEAD_OFFSET as f32,
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
        .build_ui(move |ui| {
            // 与生产 `show_bands` 同源：重建铺满视口的根 Ui，坐标从 (0,0) 起，
            // 不受 egui_kittest 默认 8px 外缩进影响（否则像素断言全是猜的）。
            let full = ui.ctx().viewport_rect();
            let mut root = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(full)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            root.set_clip_rect(full);
            piano_roll::draw_chrome(&root, &view);
        });
    harness.run();
    harness
        .render()
        .expect("卷帘骨架像素测试需要可用的 wgpu 渲染器")
}

fn alpha(image: &RgbaImage, x: u32, y: u32) -> u8 {
    image.get_pixel(x, y).0[3]
}

/// 该像素是否为演奏指示线颜色。
fn is_playhead(image: &RgbaImage, x: u32, y: u32) -> bool {
    let expected = lumino_ui_egui::theme::playhead();
    let pixel = image.get_pixel(x, y).0;
    pixel[..3] == expected.to_array()[..3] && pixel[3] > 0
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

    // ── 演奏指示线：必须纵贯网格整高，且在骨架最后一层（不被标尺/滚动条盖住）──
    let playhead_x = KEYBOARD_W + PLAYHEAD_OFFSET;
    assert!(
        is_playhead(&image, playhead_x, 10),
        "指示线必须出现在标尺行（线头）：pixel={:?}",
        image.get_pixel(playhead_x, 10).0
    );
    assert!(
        is_playhead(&image, playhead_x, 300),
        "指示线必须纵贯网格中部——只在标尺行内一段 = 被卷帘内容盖住的历史回归"
    );
    assert!(
        is_playhead(&image, playhead_x, HEIGHT - SCROLLBAR - 2),
        "指示线必须画到水平滚动条上沿"
    );
    assert!(
        !is_playhead(&image, playhead_x, HEIGHT - 2),
        "指示线不得压到水平滚动条"
    );
    // 顶部 8px 倒三角：y=3 行的线宽必须明显大于 2px 线身。
    let triangle_w = (playhead_x - 5..=playhead_x + 5)
        .filter(|&x| is_playhead(&image, x, 3))
        .count();
    assert!(
        triangle_w >= 4,
        "标尺顶部必须有 8px 倒三角（y=3 行宽 ≥4px），实际 {triangle_w}px"
    );
    assert!(!is_playhead(&image, 790, 300), "指示线不得压到垂直滚动条");
    assert!(!is_playhead(&image, 10, 300), "指示线不得画进键盘列");

    // 网格其余区域仍保持透明：底下的 gfx 音符纹理必须透出来。
    assert_eq!(alpha(&image, 400, 300), 0, "中央网格区域必须保持透明");

    // ── 演奏键色：`PianoRollView::key_colors` 必须真的叠到对应键上 ──
    //
    // 回归点：键色数据曾只停留在 Editor 内部（`update_playback_key_colors`
    // 照常计算），但 egui 键盘列完全不读它 —— 打开「播放时键盘颜色指示」
    // 后键盘毫无变化。这里锁死「数据 → 像素」的接线。
    let plain = image;
    let mut tinted = view();
    // 默认视图 key_height=20 / scroll_y=0 / ruler=24 → key 120 占 y=164..184。
    let offset = 120 * 4;
    tinted.key_colors[offset..offset + 4].copy_from_slice(&[255, 0, 0, 255]);
    let colored = render(tinted);
    let baseline = plain.get_pixel(10, 174).0;
    let highlighted = colored.get_pixel(10, 174).0;
    assert_ne!(
        highlighted, baseline,
        "打开键色后对应键必须被音轨色覆盖：base={baseline:?} key={highlighted:?}"
    );
    assert!(
        highlighted[1] < baseline[1] && highlighted[2] < baseline[2],
        "红色洋葱皮必须压低键底色的 G/B 通道：base={baseline:?} key={highlighted:?}"
    );
    assert_eq!(
        colored.get_pixel(10, 194).0,
        plain.get_pixel(10, 194).0,
        "未发声的相邻键不得变色"
    );
}
