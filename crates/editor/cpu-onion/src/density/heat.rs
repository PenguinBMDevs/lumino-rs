//! 蓝色热力色带与「计数 → 颜色」映射。

use std::sync::LazyLock;

/// 蓝色热力色带（sRGB display 空间插值）。
///
/// 示例的低端色 `[11, 13, 16]` 与深色背景同色，用于隐藏低密度；生产是透明
/// 叠加层且浅色主题下会变成近黑噪点，故低端改为可见深蓝。
static HEAT_LUT: LazyLock<[u32; 256]> = LazyLock::new(|| {
    const STOPS: [(f32, [u8; 3]); 4] = [
        (0.0, [22, 58, 95]),
        (0.45, [42, 111, 219]),
        (0.75, [111, 195, 255]),
        (1.0, [245, 250, 255]),
    ];
    let mut lut = [0u32; 256];
    for (index, slot) in lut.iter_mut().enumerate() {
        let t = index as f32 / 255.0;
        let mut left = STOPS[0];
        let mut right = STOPS[STOPS.len() - 1];
        for pair in STOPS.windows(2) {
            if t >= pair[0].0 && t <= pair[1].0 {
                left = pair[0];
                right = pair[1];
                break;
            }
        }
        let span = (right.0 - left.0).max(0.000_001);
        let local = ((t - left.0) / span).clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| -> u8 {
            (f32::from(a) + (f32::from(b) - f32::from(a)) * local).round() as u8
        };
        *slot = 0xFF00_0000
            | ((mix(left.1[0], right.1[0]) as u32) << 16)
            | ((mix(left.1[1], right.1[1]) as u32) << 8)
            | mix(left.1[2], right.1[2]) as u32;
    }
    lut
});

/// 密度计数 → 不透明 `0xAARRGGBB`；`count == 0` 返回 0（透明）。
///
/// 用 `log2` 归一化：黑乐谱的密集段与稀疏段差几个数量级，线性映射只有少数
/// 像素可见。
#[must_use]
pub fn heat_argb(count: u16, max_count: u16) -> u32 {
    let count = u32::from(count);
    if count == 0 {
        return 0;
    }
    let numerator = count.ilog2() as f32;
    let denominator = u32::from(max_count.max(1)).ilog2().max(1) as f32;
    let index = ((numerator / denominator).clamp(0.0, 1.0) * 255.0) as usize;
    HEAT_LUT[index.min(255)]
}
