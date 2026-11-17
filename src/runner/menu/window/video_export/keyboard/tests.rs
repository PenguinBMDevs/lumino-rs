use super::*;

#[test]
fn test_composite_keyboard_colors_blend() {
    const WIDTH: u32 = 60;
    // 使用 zoom_y=2，确保最高键（key=127）至少有两行像素，
    // 可避开白键底部的 1px 边框行（基础色为浅灰而非纯白）。
    const HEIGHT: u32 = 30 + 256; // ruler 30 + 128 键 × 2
    let (kb_pixels, kb_w, kb_h) = generate_keyboard_texture(WIDTH, HEIGHT, 128);
    let mut frame = vec![0u8; (WIDTH * HEIGHT * 4) as usize];

    let mut key_colors = [0u8; KEY_COLOR_BYTES];
    // 将最高键（key=127，顶部 py=0）着为纯红
    key_colors[127 * 4] = 255;
    key_colors[127 * 4 + 3] = 255;

    composite_keyboard(
        &mut frame,
        WIDTH,
        HEIGHT,
        &kb_pixels,
        kb_w,
        kb_h,
        &key_colors,
    );

    // ruler 下方第一行（key=127 的非边框行）最左侧像素应被混合为红色
    let idx = (30 * WIDTH as usize) * 4;
    // 白键基础色为 (255,255,255)，叠加 (255,0,0) × 0.6 → (102,102,255)
    assert_eq!(frame[idx], 102, "B 通道应被混合");
    assert_eq!(frame[idx + 1], 102, "G 通道应被混合");
    assert_eq!(frame[idx + 2], 255, "R 通道应保持 255");
    assert_eq!(frame[idx + 3], 255, "Alpha 应为不透明");
}
