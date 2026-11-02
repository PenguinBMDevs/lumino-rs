//! 输出帧：透明背景的 sRGB RGBA8。
//!
//! 约定：
//! - `pixels` 为行优先 RGBA8，长度 = `width * height * 4`；
//! - 背景像素为 `[0, 0, 0, 0]`（alpha = 0，完全透明）；
//! - 音符填充/边框像素 alpha = 255，RGB 为 linear → sRGB 编码后的值；
//! - 本 crate 只负责 canvas 区域，帧内不包含键盘栏 / 标尺 / 网格 / 背景色。
//!
//! 像素用 `Arc<Vec<u8>>` 持有：整帧快照只克隆 Arc，渲染器写回时由
//! `Arc::make_mut` 做写时复制，保证已发出的旧快照不会被原地改写。

use std::sync::Arc;

/// 一帧 CPU 光栅结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CpuFrame {
    /// 宽度（像素）。
    pub width: u32,
    /// 高度（像素）。
    pub height: u32,
    /// 行优先 RGBA8 像素。
    pub pixels: Arc<Vec<u8>>,
}

impl CpuFrame {
    /// 创建全透明帧。
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let len = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        Self {
            width,
            height,
            pixels: Arc::new(vec![0; len]),
        }
    }

    /// 读取一个像素；越界返回 `None`。
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let offset = (y as usize)
            .checked_mul(self.width as usize)?
            .checked_add(x as usize)?
            .checked_mul(4)?;
        let end = offset.checked_add(4)?;
        let bytes: [u8; 4] = self.pixels.get(offset..end)?.try_into().ok()?;
        Some(bytes)
    }

    /// 是否为空帧（宽或高为 0）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_frame_is_transparent() {
        let frame = CpuFrame::new(3, 2);
        assert_eq!(frame.pixels.len(), 3 * 2 * 4);
        assert!(frame.pixels.iter().all(|byte| *byte == 0));
        assert_eq!(frame.pixel(0, 0), Some([0, 0, 0, 0]));
        assert_eq!(frame.pixel(2, 1), Some([0, 0, 0, 0]));
        assert_eq!(frame.pixel(3, 1), None);
        assert_eq!(frame.pixel(0, 2), None);
    }

    #[test]
    fn empty_frame_has_no_pixels() {
        assert!(CpuFrame::new(0, 10).is_empty());
        assert!(CpuFrame::new(10, 0).is_empty());
        assert!(!CpuFrame::new(1, 1).is_empty());
    }

    #[test]
    fn clone_is_copy_on_write() {
        let mut frame = CpuFrame::new(1, 1);
        Arc::make_mut(&mut frame.pixels)[0] = 7;
        let snapshot = frame.clone();
        Arc::make_mut(&mut frame.pixels)[0] = 9;
        assert_eq!(snapshot.pixel(0, 0), Some([7, 0, 0, 0]));
        assert_eq!(frame.pixel(0, 0), Some([9, 0, 0, 0]));
    }
}
