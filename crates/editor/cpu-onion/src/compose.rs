//! 帧合成 / 水平平移 / 内部 `0xAARRGGBB` → 输出 RGBA8。
//!
//! 与渲染器解耦为纯函数，便于单测「平移 + 输出同步」和「ARGB → RGBA」语义。
//! `shift` 同时维护内部 u32 帧与对外 RGBA8 帧，保证 `frame()` 在平移后立即
//! 与相机一致（不依赖下一次窄条渲染才刷新）。

use crate::{buffer::BandBuffer, frame::CpuFrame};

/// 把一条 band 的像素拷进内部合成帧。
pub(crate) fn compose_band(
    frame: &mut [u32],
    width: usize,
    height: usize,
    x0: u32,
    x1: u32,
    buffer: &BandBuffer,
) {
    let band_width = x1.saturating_sub(x0) as usize;
    if band_width == 0
        || buffer.width != band_width
        || buffer.height != height
        || frame.len() != width.saturating_mul(height)
    {
        return;
    }
    let x0 = x0 as usize;
    if x0.saturating_add(band_width) > width {
        return;
    }
    for y in 0..height {
        let src_start = y * band_width;
        let dst_start = y * width + x0;
        let (Some(src), Some(dst)) = (
            buffer.pixels.get(src_start..src_start + band_width),
            frame.get_mut(dst_start..dst_start + band_width),
        ) else {
            continue;
        };
        dst.copy_from_slice(src);
    }
}

/// 把内部合成帧的 `[x0, x1)` 列转换为 RGBA8 输出。
///
/// 背景 `0x00000000` → `[0, 0, 0, 0]`；音符 `0xFFRRGGBB` → `[R, G, B, 255]`。
pub(crate) fn write_output(
    frame: &[u32],
    width: usize,
    height: usize,
    output: &mut CpuFrame,
    x0: u32,
    x1: u32,
) {
    let x0 = (x0 as usize).min(width);
    let x1 = (x1 as usize).min(width);
    if x0 >= x1 || width == 0 || height == 0 {
        return;
    }
    let pixel_count = width.saturating_mul(height);
    if frame.len() != pixel_count
        || output.width != width as u32
        || output.height != height as u32
        || output.pixels.len() != pixel_count.saturating_mul(4)
    {
        return;
    }
    let span = x1 - x0;
    for y in 0..height {
        let src_start = y * width + x0;
        let Some(src) = frame.get(src_start..src_start + span) else {
            continue;
        };
        let dst_start = src_start * 4;
        let Some(dst) = output.pixels.get_mut(dst_start..dst_start + span * 4) else {
            continue;
        };
        for (chunk, &pixel) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src.iter()) {
            *chunk = [
                ((pixel >> 16) & 0xFF) as u8,
                ((pixel >> 8) & 0xFF) as u8,
                (pixel & 0xFF) as u8,
                ((pixel >> 24) & 0xFF) as u8,
            ];
        }
    }
}

/// 按整数像素平移内部帧与输出帧。
///
/// `dx > 0` 表示视口向右滚动（内容左移）；`|dx| >= width` 时整帧清空。
pub(crate) fn shift(frame: &mut [u32], output: &mut CpuFrame, width: u32, height: u32, dx: i32) {
    if dx == 0 {
        return;
    }
    let width = width as usize;
    let height = height as usize;
    if width == 0 || height == 0 {
        return;
    }
    let pixel_count = width.saturating_mul(height);
    if frame.len() != pixel_count || output.pixels.len() != pixel_count.saturating_mul(4) {
        return;
    }
    let shift = dx.unsigned_abs() as usize;
    if shift >= width {
        frame.fill(0);
        output.clear();
        return;
    }

    let byte_stride = width * 4;
    if dx > 0 {
        for y in 0..height {
            if let Some(row) = frame.get_mut(y * width..(y + 1) * width) {
                row.copy_within(shift..width, 0);
                row[width - shift..].fill(0);
            }
            if let Some(row) = output
                .pixels
                .get_mut(y * byte_stride..(y + 1) * byte_stride)
            {
                row.copy_within(shift * 4..byte_stride, 0);
                row[(width - shift) * 4..].fill(0);
            }
        }
    } else {
        for y in 0..height {
            if let Some(row) = frame.get_mut(y * width..(y + 1) * width) {
                row.copy_within(0..width - shift, shift);
                row[..shift].fill(0);
            }
            if let Some(row) = output
                .pixels
                .get_mut(y * byte_stride..(y + 1) * byte_stride)
            {
                row.copy_within(0..(width - shift) * 4, shift * 4);
                row[..shift * 4].fill(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_positive_keeps_u32_and_rgba_in_sync() {
        let mut frame = vec![1u32, 2, 3, 4, 5, 6, 7, 8];
        let mut output = CpuFrame::new(4, 2);
        for (index, pixel) in frame.iter().enumerate() {
            let rgba = crate::color::argb_to_rgba(*pixel);
            output.pixels[index * 4..index * 4 + 4].copy_from_slice(&rgba);
        }
        shift(&mut frame, &mut output, 4, 2, 1);
        assert_eq!(frame, vec![2, 3, 4, 0, 6, 7, 8, 0]);
        assert_eq!(output.pixel(0, 0), Some([0, 0, 2, 0]));
        assert_eq!(output.pixel(2, 0), Some([0, 0, 4, 0]));
        assert_eq!(output.pixel(3, 0), Some([0, 0, 0, 0]));
        assert_eq!(output.pixel(0, 1), Some([0, 0, 6, 0]));
    }

    #[test]
    fn shift_negative_keeps_u32_and_rgba_in_sync() {
        let mut frame = vec![1u32, 2, 3, 4, 5, 6, 7, 8];
        let mut output = CpuFrame::new(4, 2);
        for (index, pixel) in frame.iter().enumerate() {
            let rgba = crate::color::argb_to_rgba(*pixel);
            output.pixels[index * 4..index * 4 + 4].copy_from_slice(&rgba);
        }
        shift(&mut frame, &mut output, 4, 2, -1);
        assert_eq!(frame, vec![0, 1, 2, 3, 0, 5, 6, 7]);
        assert_eq!(output.pixel(0, 0), Some([0, 0, 0, 0]));
        assert_eq!(output.pixel(1, 0), Some([0, 0, 1, 0]));
        assert_eq!(output.pixel(0, 1), Some([0, 0, 0, 0]));
        assert_eq!(output.pixel(1, 1), Some([0, 0, 5, 0]));
    }

    #[test]
    fn shift_beyond_width_clears_frame() {
        let mut frame = vec![7u32; 1];
        let mut output = CpuFrame::new(1, 1);
        output.pixels.copy_from_slice(&[1, 2, 3, 4]);
        shift(&mut frame, &mut output, 1, 1, 5);
        assert_eq!(frame, vec![0]);
        assert_eq!(output.pixel(0, 0), Some([0, 0, 0, 0]));
    }

    #[test]
    fn write_output_maps_argb_to_rgba() {
        let frame = vec![0u32, 0xFF11_2233, 0x0000_0000, 0xFF44_5566];
        let mut output = CpuFrame::new(2, 2);
        write_output(&frame, 2, 2, &mut output, 0, 2);
        assert_eq!(output.pixel(0, 0), Some([0, 0, 0, 0]));
        assert_eq!(output.pixel(1, 0), Some([0x11, 0x22, 0x33, 0xFF]));
        assert_eq!(output.pixel(0, 1), Some([0, 0, 0, 0]));
        assert_eq!(output.pixel(1, 1), Some([0x44, 0x55, 0x66, 0xFF]));
    }

    #[test]
    fn compose_band_only_copies_requested_columns() {
        let buffer = BandBuffer {
            pixels: vec![9, 9, 9, 9],
            owner: vec![1, 1, 1, 1],
            width: 2,
            height: 2,
        };
        let mut frame = vec![0u32; 6];
        compose_band(&mut frame, 3, 2, 1, 3, &buffer);
        assert_eq!(frame, vec![0, 9, 9, 0, 9, 9]);
    }
}
