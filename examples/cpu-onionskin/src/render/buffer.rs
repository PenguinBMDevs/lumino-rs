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
    /// 列主序音符层（`x * height + y`）。窄音符在列主序下写 20 个连续像素，
    /// 避免行主序每行一次 cache line；最后一次性合成回 `pixels`。
    pub column_pixels: Vec<u32>,
    /// 列主序 owner。
    pub column_owner: Vec<u32>,
    /// 已被 owner>0 覆盖的像素数（只统计画布行，不含标尺）。
    ///
    /// 用于 Exact 的提前退出：洋葱皮 pass 一旦把画布铺满，后续洋葱皮轨
    /// 不可能再改变任何像素（同级先到先得），可以整轨跳过。
    pub covered_pixels: usize,
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
        self.column_pixels = vec![0; width * height];
        self.column_owner = vec![0; width * height];
        self.covered_pixels = 0;
    }

    /// 清空 owner（背景填充由 chrome 负责）。
    pub fn clear_owner(&mut self) {
        self.owner.fill(0);
        self.column_owner.fill(0);
        self.covered_pixels = 0;
    }

    /// 把列主序音符层合成进行主序（只覆盖有 owner 的像素）。
    pub fn composite_columns(&mut self) {
        let width = self.width;
        let height = self.height;
        for y in 0..height {
            let row = &mut self.pixels[y * width..(y + 1) * width];
            for (x, pixel) in row.iter_mut().enumerate() {
                let index = x * height + y;
                if self.column_owner[index] != 0 {
                    *pixel = self.column_pixels[index];
                }
            }
        }
    }

    /// 某行像素切片。
    #[inline]
    pub fn row(&mut self, y: usize) -> &mut [u32] {
        let start = y * self.width;
        &mut self.pixels[start..start + self.width]
    }
}
