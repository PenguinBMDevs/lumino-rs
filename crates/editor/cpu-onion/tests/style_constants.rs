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
    assert!(onion.contains("let horiz_margin = 1.0 / half_width_pixels * border_px;"));
    assert!(onion.contains("let vert_margin = 1.0 / half_height_pixels * border_px;"));
    assert!(onion.contains("color = color * BORDER_DARKEN_FACTOR;"));
    assert!(onion.contains("return vec4<f32>(color, 1.0);"));

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
