/// 将 `color` 按 `alpha`（0-255）混合到 BGRA 像素上（保留背景透明度不变）。
pub(super) fn blend_pixel(dst: &mut [u8], color: [u8; 4], alpha: u32) {
    let a = alpha;
    let inv = 255 - a;
    dst[0] = ((color[0] as u32 * a + dst[0] as u32 * inv) / 255) as u8;
    dst[1] = ((color[1] as u32 * a + dst[1] as u32 * inv) / 255) as u8;
    dst[2] = ((color[2] as u32 * a + dst[2] as u32 * inv) / 255) as u8;
}
