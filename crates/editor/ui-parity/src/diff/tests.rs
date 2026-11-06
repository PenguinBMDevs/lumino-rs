//! `diff` 模块单元测试。

use image::{Rgba, RgbaImage};

use super::{CompareOptions, compare_rgba};

fn solid(width: u32, height: u32, color: [u8; 4]) -> RgbaImage {
    RgbaImage::from_pixel(width, height, Rgba(color))
}

fn image_with_block(block_x: u32) -> RgbaImage {
    let mut image = solid(32, 32, [0, 0, 0, 255]);
    for y in 8..16 {
        for x in block_x..block_x + 8 {
            image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
        }
    }
    image
}

fn options(color_tolerance: u8, geometry_tolerance: u8) -> CompareOptions {
    CompareOptions {
        color_tolerance,
        geometry_tolerance,
    }
}

#[test]
fn test_identical_images_match() {
    let a = image_with_block(8);
    let b = a.clone();
    let result = compare_rgba(&a, &b, &options(0, 0)).expect("相同尺寸应可比较");

    assert!(result.report.matched);
    assert_eq!(result.report.pixels_over_tolerance, 0);
    assert_eq!(result.report.offset_x, 0);
    assert_eq!(result.report.offset_y, 0);
    assert_eq!(result.report.max_channel_delta, 0);
}

#[test]
fn test_color_tolerance() {
    let a = solid(4, 4, [10, 10, 10, 255]);
    let b = solid(4, 4, [12, 12, 12, 255]);

    let strict = compare_rgba(&a, &b, &options(1, 0)).expect("相同尺寸应可比较");
    assert!(!strict.report.matched);
    assert_eq!(strict.report.pixels_over_tolerance, 16);
    assert_eq!(strict.report.max_channel_delta, 2);

    let relaxed = compare_rgba(&a, &b, &options(2, 0)).expect("相同尺寸应可比较");
    assert!(relaxed.report.matched);
    assert_eq!(relaxed.report.pixels_over_tolerance, 0);
    assert_eq!(relaxed.report.different_pixels, 16);
}

#[test]
fn test_geometry_tolerance_finds_shift() {
    let a = image_with_block(8);
    let b = image_with_block(10);

    let unshifted = compare_rgba(&a, &b, &options(0, 0)).expect("相同尺寸应可比较");
    assert!(!unshifted.report.matched);
    assert_eq!(unshifted.report.offset_x, 0);

    let shifted = compare_rgba(&a, &b, &options(0, 2)).expect("相同尺寸应可比较");
    assert_eq!(shifted.report.offset_x, -2, "B 的内容右移 2px，应对齐到 -2");
    assert_eq!(shifted.report.offset_y, 0);
    // 平移后内容完全对齐；左侧 2 列成为无法比较的边界。
    assert_eq!(shifted.report.different_pixels, 0);
    assert_eq!(shifted.report.uncovered_pixel_count, 2 * 32);
    assert_eq!(shifted.report.pixels_over_tolerance, 2 * 32);
    assert!(!shifted.report.matched, "边界缺口仍应计入差异");
}

#[test]
fn test_size_mismatch_is_rejected() {
    let a = solid(4, 4, [0, 0, 0, 255]);
    let b = solid(5, 4, [0, 0, 0, 255]);
    let error = compare_rgba(&a, &b, &options(0, 0)).expect_err("尺寸不同必须报错");
    assert!(error.to_string().contains("尺寸不一致"));
}

#[test]
fn test_diff_heatmap_marks_direction() {
    let a = solid(2, 2, [200, 0, 0, 255]);
    let b = solid(2, 2, [0, 0, 200, 255]);
    let result = compare_rgba(&a, &b, &options(0, 0)).expect("相同尺寸应可比较");

    let pixel = result.diff.get_pixel(0, 0);
    assert_eq!(pixel[0], 255, "A 更亮应标红");
    assert_eq!(pixel[1], 0);
    assert_eq!(pixel[2], 255, "B 更亮应标蓝");
}
