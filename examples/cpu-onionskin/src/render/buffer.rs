//! 每 band 的像素/owner 缓冲。
//!
//! `pixels` 为 0x00RRGGBB（softbuffer 口径）；`owner` 为深度优先级：
//! 0=空、1=洋葱皮、2=主轨。band 之间互不重叠，合成时按 x 拷贝。

/// 一条横向 band 的输出缓冲。
#[derive(Debug, Default)]
pub struct BandBuffer {
    /// 像素（0x00RRGGBB）。
    pub pixels: Vec<u32>,
    /// 每像素 owner（0/1/2）。
    pub owner: Vec<u32>,
    /// band 宽度（像素）。
    pub width: usize,
    /// band 高度（= 窗口高度）。
    pub height: usize,
}

impl BandBuffer {
    /// 保证缓冲尺寸；尺寸变化时重建。
    pub fn ensure(&mut self, width: usize, height: usize) {
        if self.width == width && self.height == height && self.pixels.len() == width * height {
            return;
        }
        self.width = width;
        self.height = height;
        self.pixels = vec![0; width * height];
        self.owner = vec![0; width * height];
    }

    /// 清空 owner（背景填充由 chrome 负责）。
    pub fn clear_owner(&mut self) {
        self.owner.fill(0);
    }

    /// 某行像素切片。
    #[inline]
    pub fn row(&mut self, y: usize) -> &mut [u32] {
        let start = y * self.width;
        &mut self.pixels[start..start + self.width]
    }
}
