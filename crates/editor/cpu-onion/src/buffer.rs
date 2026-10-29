//! 每 band 的像素 / owner 缓冲。
//!
//! `pixels` 为内部 `0xAARRGGBB`（洋葱皮-only 时只有透明背景与不透明音符两种
//! alpha）；`owner` 为覆盖优先级：0 = 空、1 = 洋葱皮。band 之间互不重叠，
//! 合成时按 x 区间拷贝。

/// 一条横向 band 的输出缓冲。
#[derive(Debug, Default)]
pub(crate) struct BandBuffer {
    /// 像素（`0xAARRGGBB`）。
    pub(crate) pixels: Vec<u32>,
    /// 每像素 owner（0 = 空，1 = 洋葱皮）。
    pub(crate) owner: Vec<u32>,
    /// band 宽度（像素）。
    pub(crate) width: usize,
    /// band 高度（像素）。
    pub(crate) height: usize,
}

impl BandBuffer {
    /// 保证缓冲尺寸；尺寸变化时重建。
    pub(crate) fn ensure(&mut self, width: usize, height: usize) {
        let len = width.saturating_mul(height);
        if self.width == width && self.height == height && self.pixels.len() == len {
            return;
        }
        self.width = width;
        self.height = height;
        self.pixels = vec![0; len];
        self.owner = vec![0; len];
    }

    /// 清空为透明背景 + 空 owner（洋葱皮无背景绘制，必须显式清像素）。
    pub(crate) fn clear(&mut self) {
        self.pixels.fill(0);
        self.owner.fill(0);
    }
}
