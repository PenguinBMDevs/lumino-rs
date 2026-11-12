//! 颜色后处理测试（DRAW-030）

use super::*;

/// 构造 BGRA 图像（每像素：B, G, R, A）
fn image(pixels: &[[u8; 3]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(pixels.len() * 4);
    for [r, g, b] in pixels {
        bytes.extend_from_slice(&[*b, *g, *r, 255]);
    }
    bytes
}

fn pixel(bytes: &[u8], index: usize) -> [u8; 3] {
    [bytes[index * 4 + 2], bytes[index * 4 + 1], bytes[index * 4]]
}

fn alpha(bytes: &[u8], index: usize) -> u8 {
    bytes[index * 4 + 3]
}

#[test]
fn test_default_is_identity() {
    let mut bytes = image(&[
        [10, 20, 30],
        [40, 50, 60],
        [70, 80, 90],
        [1, 2, 3],
        [4, 5, 6],
        [7, 8, 9],
    ]);
    let before = bytes.clone();
    adjust_bgra(3, 2, 12, &mut bytes, &Adjust::default());
    assert_eq!(bytes, before, "默认参数必须逐字节不变");
}

#[test]
fn test_brightness_raises_all_channels() {
    let mut bytes = image(&[[100, 120, 140]]);
    adjust_bgra(
        1,
        1,
        4,
        &mut bytes,
        &Adjust {
            brightness: 0.5,
            ..Default::default()
        },
    );
    let [r, g, b] = pixel(&bytes, 0);
    assert!(
        r > 100 && g > 120 && b > 140,
        "亮度 +0.5 后应变亮：{r},{g},{b}"
    );
    assert_eq!(alpha(&bytes, 0), 255, "alpha 不变");
}

#[test]
fn test_brightness_negative_darkens() {
    let mut bytes = image(&[[200, 180, 160]]);
    adjust_bgra(
        1,
        1,
        4,
        &mut bytes,
        &Adjust {
            brightness: -0.4,
            ..Default::default()
        },
    );
    let [r, g, b] = pixel(&bytes, 0);
    assert!(
        r < 200 && g < 180 && b < 160,
        "亮度 -0.4 后应变暗：{r},{g},{b}"
    );
}

#[test]
fn test_contrast_increases_spread() {
    let mut bytes = image(&[[64, 64, 64], [192, 192, 192]]);
    adjust_bgra(
        2,
        1,
        8,
        &mut bytes,
        &Adjust {
            contrast: 0.5,
            ..Default::default()
        },
    );
    let dark = pixel(&bytes, 0)[0];
    let bright = pixel(&bytes, 1)[0];
    assert!(dark < 64, "暗部应更暗：{dark}");
    assert!(bright > 192, "亮部应更亮：{bright}");
}

#[test]
fn test_contrast_negative_pulls_toward_mid() {
    let mut bytes = image(&[[32, 32, 32], [224, 224, 224]]);
    adjust_bgra(
        2,
        1,
        8,
        &mut bytes,
        &Adjust {
            contrast: -0.5,
            ..Default::default()
        },
    );
    let dark = pixel(&bytes, 0)[0];
    let bright = pixel(&bytes, 1)[0];
    assert!(dark > 32, "减对比度后暗部应抬亮：{dark}");
    assert!(bright < 224, "减对比度后亮部应压暗：{bright}");
}

#[test]
fn test_gray_is_unaffected_by_saturation() {
    let mut bytes = image(&[[128, 128, 128]]);
    let before = bytes.clone();
    adjust_bgra(
        1,
        1,
        4,
        &mut bytes,
        &Adjust {
            saturation: 0.8,
            ..Default::default()
        },
    );
    assert_eq!(bytes, before, "中性灰没有色度，饱和度不应改动它");
}

#[test]
fn test_saturation_expands_color() {
    let mut bytes = image(&[[200, 50, 50]]);
    adjust_bgra(
        1,
        1,
        4,
        &mut bytes,
        &Adjust {
            saturation: 0.5,
            ..Default::default()
        },
    );
    let [r, g, b] = pixel(&bytes, 0);
    assert!(r > 200, "饱和后红通道应更高：{r}");
    assert!(g < 50 && b < 50, "饱和后绿/蓝通道应更低：{g},{b}");
}

#[test]
fn test_sharpen_amplifies_edge() {
    // 5×1：左段 64，右段 128，边界在 x=1|2
    let mut bytes = image(&[
        [64, 64, 64],
        [64, 64, 64],
        [128, 128, 128],
        [128, 128, 128],
        [128, 128, 128],
    ]);
    adjust_bgra(
        5,
        1,
        20,
        &mut bytes,
        &Adjust {
            sharpen: 0.5,
            ..Default::default()
        },
    );
    let left = pixel(&bytes, 1)[0];
    let right = pixel(&bytes, 2)[0];
    assert!(left < 64, "边缘暗侧应更暗（过冲）：{left}");
    assert!(right > 128, "边缘亮侧应更亮（过冲）：{right}");
    assert_eq!(alpha(&bytes, 1), 255, "alpha 不变");
    assert_eq!(alpha(&bytes, 2), 255, "alpha 不变");
}

#[test]
fn test_sharpen_keeps_flat_area() {
    // 均匀图：模糊等于自身，锐化不应改动任何像素
    let mut bytes = image(&[[120, 130, 140]; 9]);
    let before = bytes.clone();
    adjust_bgra(
        3,
        3,
        12,
        &mut bytes,
        &Adjust {
            sharpen: 1.0,
            ..Default::default()
        },
    );
    assert_eq!(bytes, before, "均匀区域锐化应保持原样");
}

#[test]
fn test_zero_sharpen_does_not_blur() {
    // 锐化步关闭时，边缘两侧都应保持正值方向不变：只做色调时不会出现过冲
    let mut bytes = image(&[[64, 64, 64], [128, 128, 128]]);
    adjust_bgra(
        2,
        1,
        8,
        &mut bytes,
        &Adjust {
            sharpen: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
        },
    );
    assert_eq!(pixel(&bytes, 0)[0], 64, "sharpen=0 且未调色时逐格不变");
    assert_eq!(pixel(&bytes, 1)[0], 128, "sharpen=0 且未调色时逐格不变");
}

#[test]
fn test_alpha_is_preserved() {
    let mut bytes = image(&[[10, 20, 30], [40, 50, 60]]);
    bytes[3] = 0;
    bytes[7] = 128;
    adjust_bgra(
        2,
        1,
        8,
        &mut bytes,
        &Adjust {
            brightness: 0.5,
            contrast: 0.3,
            saturation: 0.4,
            sharpen: 0.6,
        },
    );
    assert_eq!(alpha(&bytes, 0), 0, "全透明像素 alpha 不变");
    assert_eq!(alpha(&bytes, 1), 128, "半透明像素 alpha 不变");
}

#[test]
fn test_invalid_stride_is_ignored() {
    let mut bytes = image(&[[10, 20, 30], [40, 50, 60]]);
    let before = bytes.clone();
    // stride 小于 width*4：参数非法，直接返回
    adjust_bgra(
        2,
        1,
        4,
        &mut bytes,
        &Adjust {
            brightness: 0.5,
            ..Default::default()
        },
    );
    assert_eq!(bytes, before, "非法 stride 不应写入");
}

#[test]
fn test_short_buffer_is_ignored() {
    let mut bytes = image(&[[10, 20, 30], [40, 50, 60], [70, 80, 90], [1, 2, 3]]);
    bytes.truncate(8);
    let before = bytes.clone();
    adjust_bgra(
        2,
        2,
        8,
        &mut bytes,
        &Adjust {
            brightness: 0.5,
            ..Default::default()
        },
    );
    assert_eq!(bytes, before, "缓冲区不足时不应越界写入");
}

#[test]
fn test_zero_size_is_ignored() {
    let mut bytes = Vec::new();
    adjust_bgra(
        0,
        0,
        0,
        &mut bytes,
        &Adjust {
            brightness: 1.0,
            ..Default::default()
        },
    );
    assert!(bytes.is_empty());
}

#[test]
fn test_srgb_roundtrip_is_close() {
    let to_linear = srgb_to_linear_table();
    let to_srgb = linear_to_srgb_table();
    for value in [0u8, 1, 16, 64, 128, 200, 254, 255] {
        let encoded = encode(to_srgb, to_linear[value as usize]);
        let diff = i32::from(encoded) - i32::from(value);
        assert!(
            (-1..=1).contains(&diff),
            "sRGB 往返误差应 ≤ 1：{value} -> {encoded}"
        );
    }
}
