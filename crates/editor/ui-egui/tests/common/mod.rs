//! `dialog_bands` / `settings_bands` 共用的像素断言工具。

#![allow(dead_code)]

use image::RgbaImage;

/// 墨迹区间的允许偏差（像素）。
pub const TOLERANCE: i64 = 4;

/// 某一行带内与背景差异超过阈值的列区间 `(start, end)`（含端点）。
pub fn ink_runs(
    image: &RgbaImage,
    y0: u32,
    y1: u32,
    x0: u32,
    x1: u32,
    bg: [u8; 3],
) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let mut start: Option<u32> = None;
    for x in x0..=x1 {
        let mut ink = false;
        for y in y0..=y1 {
            let pixel = image.get_pixel(x, y).0;
            let delta = (0..3)
                .map(|channel| i32::from(pixel[channel]) - i32::from(bg[channel]))
                .map(i32::abs)
                .sum::<i32>();
            if delta > 40 {
                ink = true;
                break;
            }
        }
        match (ink, start) {
            (true, None) => start = Some(x),
            (false, Some(from)) => {
                runs.push((from, x - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        runs.push((from, x1));
    }
    runs
}

/// 某一列带内与背景差异超过阈值的行区间。
pub fn ink_row_runs(
    image: &RgbaImage,
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
    bg: [u8; 3],
) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let mut start: Option<u32> = None;
    for y in y0..=y1 {
        let mut ink = false;
        for x in x0..=x1 {
            let pixel = image.get_pixel(x, y).0;
            let delta = (0..3)
                .map(|channel| i32::from(pixel[channel]) - i32::from(bg[channel]))
                .map(i32::abs)
                .sum::<i32>();
            if delta > 40 {
                ink = true;
                break;
            }
        }
        match (ink, start) {
            (true, None) => start = Some(y),
            (false, Some(from)) => {
                runs.push((from, y - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        runs.push((from, y1));
    }
    runs
}

pub fn assert_runs_close(actual: &[(u32, u32)], expected: &[(u32, u32)], label: &str) {
    assert_runs_close_with(actual, expected, TOLERANCE, label);
}

pub fn assert_runs_close_with(
    actual: &[(u32, u32)],
    expected: &[(u32, u32)],
    tol: i64,
    label: &str,
) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label} 墨迹区间数量不一致：\n实际 {actual:?}\n期望 {expected:?}"
    );
    for (index, (got, want)) in actual.iter().zip(expected).enumerate() {
        let start_delta = i64::from(got.0) - i64::from(want.0);
        let end_delta = i64::from(got.1) - i64::from(want.1);
        assert!(
            start_delta.abs() <= tol && end_delta.abs() <= tol,
            "{label} 第 {index} 段偏移超限：实际 {got:?}，期望 {want:?}"
        );
    }
}
