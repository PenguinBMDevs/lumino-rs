//! 图像对比核心：逐像素通道差统计 + 红/蓝差异热图。
//!
//! 文字抗锯齿不可能跨框架逐像素一致，因此本工具**只给数据**，不做模糊判定：
//! - `pixels_over_tolerance`：任一通道差超过 `color_tolerance` 的像素数；
//! - 几何容差：允许一次全局整数平移，取差异像素最少的偏移；
//! - 差异图：A 更亮标红、B 更亮标蓝，强度按 delta×4 放大便于观察。

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use image::RgbaImage;
use rayon::prelude::*;
use serde::Serialize;

/// 几何容差上限（更大的搜索范围代价是平方级增长，不建议用于全屏基线）。
pub const MAX_GEOMETRY_TOLERANCE: u8 = 4;

/// 对比选项。
#[derive(Debug, Clone, Copy)]
pub struct CompareOptions {
    /// 单通道允许的最大差值（0..=255）。
    pub color_tolerance: u8,
    /// 允许的全局平移像素数（0 = 不平移）。
    pub geometry_tolerance: u8,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            color_tolerance: 2,
            geometry_tolerance: 1,
        }
    }
}

/// 可序列化的对比报告（`ui-diff` 的 JSON 输出主体）。
#[derive(Debug, Clone, Serialize)]
pub struct CompareReport {
    /// 图像宽度（像素）。
    pub width: u32,
    /// 图像高度（像素）。
    pub height: u32,
    /// 使用的颜色容差。
    pub color_tolerance: u8,
    /// 请求的几何容差。
    pub geometry_tolerance: u8,
    /// 实际采用的全局平移 X（B 的像素 `(x, y)` 与 A 的 `(x + offset_x, y + offset_y)` 对齐）。
    pub offset_x: i32,
    /// 实际采用的全局平移 Y。
    pub offset_y: i32,
    /// 图像总像素数。
    pub pixel_count: u64,
    /// 平移后仍可比较的重叠像素数。
    pub overlap_pixel_count: u64,
    /// 平移后落在边界外、无法比较的像素数（计入超容差）。
    pub uncovered_pixel_count: u64,
    /// 重叠区域内任一通道存在差异（delta > 0）的像素数。
    pub different_pixels: u64,
    /// 任一通道差超过 `color_tolerance` 的像素数（含 `uncovered_pixel_count`）。
    pub pixels_over_tolerance: u64,
    /// `pixels_over_tolerance / pixel_count`。
    pub pixels_over_tolerance_ratio: f64,
    /// 重叠区域内单通道最大绝对差。
    pub max_channel_delta: u8,
    /// 重叠区域内单通道平均绝对差。
    pub mean_channel_delta: f64,
    /// 是否在容差内完全一致（`pixels_over_tolerance == 0`）。
    pub matched: bool,
}

/// 对比结果：报告 + 差异热图。
#[derive(Debug)]
pub struct CompareResult {
    /// 统计数据。
    pub report: CompareReport,
    /// 红/蓝差异热图（尺寸与输入一致）。
    pub diff: RgbaImage,
}

/// 从两个 PNG 文件加载并对比。
///
/// # 错误
/// - 文件读取/解码失败；
/// - 两张图尺寸不一致。
pub fn compare_images(
    a_path: &Path,
    b_path: &Path,
    options: &CompareOptions,
) -> Result<CompareResult> {
    let a = image::open(a_path)
        .with_context(|| format!("读取图片 A 失败: {}", a_path.display()))?
        .to_rgba8();
    let b = image::open(b_path)
        .with_context(|| format!("读取图片 B 失败: {}", b_path.display()))?
        .to_rgba8();

    if a.dimensions() != b.dimensions() {
        bail!(
            "图片尺寸不一致: {} = {:?}, {} = {:?}",
            a_path.display(),
            a.dimensions(),
            b_path.display(),
            b.dimensions()
        );
    }

    compare_rgba(&a, &b, options)
}

/// 对比两张已解码的 RGBA 图像。
///
/// # 错误
/// - 尺寸不一致；
/// - 尺寸为零。
pub fn compare_rgba(
    a: &RgbaImage,
    b: &RgbaImage,
    options: &CompareOptions,
) -> Result<CompareResult> {
    let (width, height) = a.dimensions();
    if b.dimensions() != (width, height) {
        return Err(anyhow!(
            "图片尺寸不一致: A = {:?}, B = {:?}",
            a.dimensions(),
            b.dimensions()
        ));
    }
    if width == 0 || height == 0 {
        return Err(anyhow!("图片尺寸不能为零: {width}x{height}"));
    }

    let geometry_tolerance = options.geometry_tolerance.min(MAX_GEOMETRY_TOLERANCE);
    let best = find_best_offset(a, b, options.color_tolerance, geometry_tolerance);
    let report = build_report(a, b, options, geometry_tolerance, best);
    let diff = build_diff_image(a, b, options.color_tolerance, best.0, best.1);

    Ok(CompareResult { report, diff })
}

/// 根据二进制决策的全局平移结果（`(mismatch, dx, dy)`）。
#[derive(Debug, Clone, Copy)]
struct OffsetCandidate {
    mismatch: u64,
    dx: i32,
    dy: i32,
}

fn find_best_offset(
    a: &RgbaImage,
    b: &RgbaImage,
    color_tolerance: u8,
    geometry_tolerance: u8,
) -> (i32, i32) {
    let limit = i32::from(geometry_tolerance);
    let mut candidates = Vec::new();
    for dy in -limit..=limit {
        for dx in -limit..=limit {
            candidates.push((dx, dy));
        }
    }

    // 距离近的偏移优先；差异相同时避免无意义的整体位移。
    candidates.sort_by_key(|(dx, dy)| (dx.abs() + dy.abs(), dx.abs(), dy.abs(), *dx, *dy));

    let mut best: Option<OffsetCandidate> = None;
    for (dx, dy) in candidates {
        let mismatch = mismatch_count(a, b, dx, dy, color_tolerance);
        let candidate = OffsetCandidate { mismatch, dx, dy };
        if best.is_none_or(|current| candidate.mismatch < current.mismatch) {
            best = Some(candidate);
        }
    }

    best.map(|candidate| (candidate.dx, candidate.dy))
        .unwrap_or((0, 0))
}

/// 统计平移到 `(dx, dy)` 后重叠区域内不匹配的像素数。
///
/// 边界外像素不参与“最佳偏移”选择——否则小的平移会被边界惩罚压过真实的
/// 内容对齐；它们会在最终报告里单独记为 `uncovered_pixel_count`。
fn mismatch_count(a: &RgbaImage, b: &RgbaImage, dx: i32, dy: i32, tolerance: u8) -> u64 {
    let (width, height) = b.dimensions();
    let Some(bounds) = overlap_bounds(width, height, dx, dy) else {
        return u64::from(width) * u64::from(height);
    };

    let a_raw = a.as_raw();
    let b_raw = b.as_raw();
    let mismatches: u64 = (bounds.y_start..bounds.y_end)
        .into_par_iter()
        .map(|y| {
            let a_y = (y as i64 + i64::from(dy)) as usize;
            let mut row_mismatches = 0_u64;
            for x in bounds.x_start..bounds.x_end {
                let a_x = (x as i64 + i64::from(dx)) as usize;
                let a_index = (a_y * width as usize + a_x) * 4;
                let b_index = (y as usize * width as usize + x as usize) * 4;
                if !pixel_within_tolerance(
                    &a_raw[a_index..a_index + 4],
                    &b_raw[b_index..b_index + 4],
                    tolerance,
                ) {
                    row_mismatches += 1;
                }
            }
            row_mismatches
        })
        .sum();

    mismatches
}

/// 平移后的有效重叠区域。
#[derive(Debug, Clone, Copy)]
struct OverlapBounds {
    x_start: u32,
    x_end: u32,
    y_start: u32,
    y_end: u32,
}

impl OverlapBounds {
    fn area(self) -> u64 {
        u64::from(self.x_end.saturating_sub(self.x_start))
            * u64::from(self.y_end.saturating_sub(self.y_start))
    }

    fn contains(self, x: u32, y: u32) -> bool {
        x >= self.x_start && x < self.x_end && y >= self.y_start && y < self.y_end
    }
}

fn overlap_bounds(width: u32, height: u32, dx: i32, dy: i32) -> Option<OverlapBounds> {
    let x_start = u32::try_from((-i64::from(dx)).max(0)).ok()?;
    let y_start = u32::try_from((-i64::from(dy)).max(0)).ok()?;
    let x_end = width.checked_sub(u32::try_from(dx.max(0)).ok()?)?;
    let y_end = height.checked_sub(u32::try_from(dy.max(0)).ok()?)?;

    if x_start >= x_end || y_start >= y_end {
        return None;
    }

    Some(OverlapBounds {
        x_start,
        x_end,
        y_start,
        y_end,
    })
}

fn pixel_within_tolerance(a: &[u8], b: &[u8], tolerance: u8) -> bool {
    a.iter()
        .zip(b.iter())
        .all(|(a_channel, b_channel)| a_channel.abs_diff(*b_channel) <= tolerance)
}

fn build_report(
    a: &RgbaImage,
    b: &RgbaImage,
    options: &CompareOptions,
    geometry_tolerance: u8,
    offset: (i32, i32),
) -> CompareReport {
    let (width, height) = a.dimensions();
    let total = u64::from(width) * u64::from(height);
    let Some(bounds) = overlap_bounds(width, height, offset.0, offset.1) else {
        return CompareReport {
            width,
            height,
            color_tolerance: options.color_tolerance,
            geometry_tolerance,
            offset_x: offset.0,
            offset_y: offset.1,
            pixel_count: total,
            overlap_pixel_count: 0,
            uncovered_pixel_count: total,
            different_pixels: 0,
            pixels_over_tolerance: total,
            pixels_over_tolerance_ratio: 1.0,
            max_channel_delta: 0,
            mean_channel_delta: 0.0,
            matched: false,
        };
    };

    let a_raw = a.as_raw();
    let b_raw = b.as_raw();
    let mut different_pixels = 0_u64;
    let mut pixels_over_tolerance = 0_u64;
    let mut max_delta = 0_u8;
    let mut sum_delta = 0_u64;

    for y in 0..height {
        for x in 0..width {
            if !bounds.contains(x, y) {
                continue;
            }
            let a_x = (x as i64 + i64::from(offset.0)) as usize;
            let a_y = (y as i64 + i64::from(offset.1)) as usize;
            let a_index = (a_y * width as usize + a_x) * 4;
            let b_index = (y as usize * width as usize + x as usize) * 4;

            let mut pixel_max = 0_u8;
            for channel in 0..4 {
                let delta = a_raw[a_index + channel].abs_diff(b_raw[b_index + channel]);
                sum_delta += u64::from(delta);
                pixel_max = pixel_max.max(delta);
            }

            if pixel_max > 0 {
                different_pixels += 1;
            }
            if pixel_max > options.color_tolerance {
                pixels_over_tolerance += 1;
            }
            max_delta = max_delta.max(pixel_max);
        }
    }

    let overlap = bounds.area();
    let uncovered = total.saturating_sub(overlap);
    let total_over = pixels_over_tolerance.saturating_add(uncovered);

    CompareReport {
        width,
        height,
        color_tolerance: options.color_tolerance,
        geometry_tolerance,
        offset_x: offset.0,
        offset_y: offset.1,
        pixel_count: total,
        overlap_pixel_count: overlap,
        uncovered_pixel_count: uncovered,
        different_pixels,
        pixels_over_tolerance: total_over,
        pixels_over_tolerance_ratio: total_over as f64 / total as f64,
        max_channel_delta: max_delta,
        mean_channel_delta: if overlap == 0 {
            0.0
        } else {
            sum_delta as f64 / (overlap as f64 * 4.0)
        },
        matched: total_over == 0,
    }
}

fn build_diff_image(a: &RgbaImage, b: &RgbaImage, tolerance: u8, dx: i32, dy: i32) -> RgbaImage {
    let (width, height) = b.dimensions();
    let mut diff = RgbaImage::new(width, height);
    let Some(bounds) = overlap_bounds(width, height, dx, dy) else {
        for pixel in diff.pixels_mut() {
            *pixel = image::Rgba([64, 64, 64, 255]);
        }
        return diff;
    };

    let a_raw = a.as_raw();
    let b_raw = b.as_raw();

    for y in 0..height {
        for x in 0..width {
            if !bounds.contains(x, y) {
                diff.put_pixel(x, y, image::Rgba([64, 64, 64, 255]));
                continue;
            }

            let a_x = (x as i64 + i64::from(dx)) as usize;
            let a_y = (y as i64 + i64::from(dy)) as usize;
            let a_index = (a_y * width as usize + a_x) * 4;
            let b_index = (y as usize * width as usize + x as usize) * 4;

            let mut positive = 0_u8;
            let mut negative = 0_u8;
            for channel in 0..3 {
                let a_channel = a_raw[a_index + channel];
                let b_channel = b_raw[b_index + channel];
                positive = positive.max(a_channel.saturating_sub(b_channel));
                negative = negative.max(b_channel.saturating_sub(a_channel));
            }

            let color = if positive.max(negative) <= tolerance {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([heat(positive), 0, heat(negative), 255])
            };
            diff.put_pixel(x, y, color);
        }
    }

    diff
}

/// 差异强度放大 4 倍并饱和到 255，便于肉眼观察 1~3 的细微差异。
fn heat(delta: u8) -> u8 {
    delta.saturating_mul(4)
}

#[cfg(test)]
mod tests;
