//! 常量对照：CPU 洋葱皮样式必须与 GPU / UI 事实源一致。
//!
//! 这些 `include_str!` 是**源码头文件级**断言：GPU shader 或 UI 边框常量
//! 一旦改动，本测试立即失败，而不是等画面出现色差才发现。

use lumino_cpu_onion::color::{
    BORDER_DARKEN_FACTOR, MAIN_TRACK_LINEAR, NOTE_BORDER_PX, NOTE_BORDER_WIDTH, argb_to_rgba,
    linear_scaled_argb, linear_to_argb,
};

#[test]
fn style_constants_match_production_sources() {
    let onion = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../gfx/src/shaders/onion_note.wgsl"
    ));
    // 主轨蓝、边框因子、每侧 2px 的着色器算法、音符不透明输出。
    assert!(onion.contains("const MAIN_TRACK_COLOR: vec3<f32> = vec3<f32>(0.2, 0.55, 1.0);"));
    assert!(onion.contains("const BORDER_DARKEN_FACTOR: f32 = 0.4;"));
    assert!(onion.contains("let border_px = f32(input.border_width & 0xFFFFu);"));
    assert!(onion.contains("color = color * BORDER_DARKEN_FACTOR;"));
    assert!(onion.contains("return vec4<f32>(color, 1.0);"));

    // 边框判定必须是**屏幕像素空间**且等号归填充（与 CPU `fill_start`/`fill_end`
    // 同义）。UV 边距实现在半像素几何下会因重心插值噪声给出相反归属，已废弃。
    assert!(
        onion.contains("let side_px = border_px * 2.0;"),
        "每侧边框必须是 2 × border_px 像素"
    );
    assert!(
        onion.contains("let local = input.position.xy - input.rect_min;"),
        "边框判定必须使用片元中心与矩形左上角的像素偏移（不得回到 UV 插值）"
    );
    assert!(
        onion.contains("local.x < side_px")
            && onion.contains("local.x > input.screen_size.x - side_px")
            && onion.contains("local.y < side_px")
            && onion.contains("local.y > input.screen_size.y - side_px"),
        "四边判定必须用严格不等（等号归填充）"
    );
    assert!(
        onion.contains("@interpolate(flat) rect_min: vec2<f32>"),
        "rect_min 必须是 flat varying（逐实例常量，不参与插值）"
    );
    for forbidden in ["horiz_margin", "vert_margin", "half_width_pixels"] {
        assert!(
            !onion.contains(forbidden),
            "onion_note.wgsl 仍残留 UV 边距实现片段：{forbidden}"
        );
    }

    let onion_skin = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../ui/src/host/render/onion_skin.rs"
    ));
    assert!(onion_skin.contains("const ONION_SKIN_BORDER_WIDTH: u32 = 1;"));

    let note_update = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../ui/src/host/render/separate_thread/runner/note_update.rs"
    ));
    assert!(note_update.contains("const MAIN_TRACK_BORDER_WIDTH: u32 = 1;"));

    // CPU 侧常量与上述事实源逐项对齐。
    assert_eq!(NOTE_BORDER_WIDTH, 1);
    assert!((NOTE_BORDER_PX - 2.0).abs() < f32::EPSILON);
    assert!((BORDER_DARKEN_FACTOR - 0.4).abs() < f32::EPSILON);
    assert_eq!(MAIN_TRACK_LINEAR, [0.2, 0.55, 1.0]);
    assert_eq!(
        &argb_to_rgba(linear_to_argb(MAIN_TRACK_LINEAR))[..3],
        &[124, 196, 255]
    );
    assert_eq!(
        &argb_to_rgba(linear_scaled_argb(MAIN_TRACK_LINEAR, BORDER_DARKEN_FACTOR))[..3],
        &[80, 129, 170]
    );
}
