//! 图片转 MIDI 的颜色后处理（DRAW-030）
//!
//! `i2m-rs` 只提供加载/聚类/抖动/重采样，没有亮度/对比度/饱和度/锐化。
//! 这里在**调色板生成之前**处理图像：调色板必须按调整后的颜色聚类，
//! 否则用户调亮图片后，颜色仍按原图取样，观感反馈是断的。
//!
//! # 口径
//!
//! - 颜色运算一律在**线性光**下做（sRGB 编解码用查表，避免逐像素 `powf`）。
//! - 通道顺序跟随 `i2m-rs` 的 `RgbaImage` 内存布局：**B, G, R, A**。
//! - alpha 原样保留；透明像素的 RGB 也照常处理（`convert` 会按 alpha 丢弃）。
//! - `Adjust::default()` 直接返回，逐字节不变（回归底线）。

use std::sync::OnceLock;

/// 亮度 / 对比度 / 饱和度 / 锐化
///
/// 取值范围：`brightness` / `contrast` / `saturation` 为 `-1.0..=1.0`，
/// `sharpen` 为 `0.0..=1.0`。超出范围按边界夹取。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adjust {
    /// 亮度：线性光下的加性偏移
    pub brightness: f32,
    /// 对比度：以 sRGB 中灰为轴心的缩放
    pub contrast: f32,
    /// 饱和度：线性光下相对亮度的缩放
    pub saturation: f32,
    /// 锐化：反锐化掩模强度（3×3 盒式模糊）
    pub sharpen: f32,
}

impl Default for Adjust {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            sharpen: 0.0,
        }
    }
}

impl Adjust {
    /// 是否为恒等变换（用于跳过整趟处理，保证逐字节不变）
    pub fn is_identity(&self) -> bool {
        self.brightness == 0.0
            && self.contrast == 0.0
            && self.saturation == 0.0
            && self.sharpen <= 0.0
    }
}

/// 对 BGRA 缓冲区应用颜色调整（原地）
///
/// `stride` 为每行字节数（`i2m-rs` 恒为 `width * 4`）。参数非法时直接返回，
/// 不做任何修改（不 panic）。
pub fn adjust_bgra(width: u32, height: u32, stride: usize, pixels: &mut [u8], adjust: &Adjust) {
    if width == 0 || height == 0 || stride < width as usize * 4 {
        return;
    }
    if pixels.len() < stride * height as usize {
        return;
    }
    if adjust.is_identity() {
        return;
    }

    // 第一阶段：色调（亮度 / 对比度 / 饱和度），线性光
    // 三项都为 0 时整步跳过：避免「sRGB→线性→sRGB」往返把像素改 ±1，
    // 也保证「只开锐化」时未受锐化影响的格子逐字节不变。
    if adjust.brightness != 0.0 || adjust.contrast != 0.0 || adjust.saturation != 0.0 {
        apply_tone(width, height, stride, pixels, adjust);
    }

    // 第二阶段：锐化。`sharpen <= 0` 时整步跳过，逐格不变。
    if adjust.sharpen > 0.0 {
        apply_sharpen(width, height, stride, pixels, adjust.sharpen.min(1.0));
    }
}

/// sRGB 中灰的线性值：对比度的固定轴心
///
/// 取 sRGB 0.5 而不是线性 0.5，否则「加对比度」会把整个中灰画面压黑。
fn contrast_pivot() -> f32 {
    srgb_to_linear_table()[128]
}

/// 亮度 / 对比度 / 饱和度（线性光，逐像素）
fn apply_tone(width: u32, height: u32, stride: usize, pixels: &mut [u8], adjust: &Adjust) {
    let to_linear = srgb_to_linear_table();
    let to_srgb = linear_to_srgb_table();
    let pivot = contrast_pivot();
    let contrast = 1.0 + adjust.contrast.clamp(-1.0, 1.0);
    let saturation = 1.0 + adjust.saturation.clamp(-1.0, 1.0);
    let brightness = adjust.brightness.clamp(-1.0, 1.0);

    for y in 0..height as usize {
        let row = y * stride;
        for x in 0..width as usize {
            let offset = row + x * 4;
            let b = to_linear[pixels[offset] as usize];
            let g = to_linear[pixels[offset + 1] as usize];
            let r = to_linear[pixels[offset + 2] as usize];

            // 对比度围绕 sRGB 中灰的线性值缩放
            let mut r = (r - pivot) * contrast + pivot;
            let mut g = (g - pivot) * contrast + pivot;
            let mut b = (b - pivot) * contrast + pivot;

            // 亮度是加性偏移
            r += brightness;
            g += brightness;
            b += brightness;

            // 饱和度：向线性亮度靠拢或远离
            if saturation != 1.0 {
                let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                r = luma + (r - luma) * saturation;
                g = luma + (g - luma) * saturation;
                b = luma + (b - luma) * saturation;
            }

            pixels[offset] = encode(to_srgb, b);
            pixels[offset + 1] = encode(to_srgb, g);
            pixels[offset + 2] = encode(to_srgb, r);
        }
    }
}

/// 反锐化掩模：`c + k·(c - blur3x3)`
fn apply_sharpen(width: u32, height: u32, stride: usize, pixels: &mut [u8], sharpen: f32) {
    let to_linear = srgb_to_linear_table();
    let to_srgb = linear_to_srgb_table();
    let w = width as usize;
    let h = height as usize;

    // 先转线性光并缓存（锐化需要邻域，不能原地边读边写）
    let mut linear: Vec<[f32; 3]> = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = y * stride;
        for x in 0..w {
            let offset = row + x * 4;
            linear.push([
                to_linear[pixels[offset] as usize],
                to_linear[pixels[offset + 1] as usize],
                to_linear[pixels[offset + 2] as usize],
            ]);
        }
    }

    for y in 0..h {
        let row = y * stride;
        for x in 0..w {
            // 3×3 盒式模糊，边界按最近像素夹取
            let mut sum = [0.0f32; 3];
            for dy in -1i32..=1 {
                let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                for dx in -1i32..=1 {
                    let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                    let sample = linear[sy * w + sx];
                    sum[0] += sample[0];
                    sum[1] += sample[1];
                    sum[2] += sample[2];
                }
            }
            let blur = [sum[0] / 9.0, sum[1] / 9.0, sum[2] / 9.0];
            let centre = linear[y * w + x];
            let offset = row + x * 4;
            // 锐化量可忽略时保留原字节：均匀区域不应因为 LUT 往返而抖动 ±1
            for (channel, slot) in [(2usize, 0usize), (1, 1), (0, 2)] {
                let value = sharpen_channel(centre[slot], blur[slot], sharpen);
                if (value - centre[slot]).abs() > 1e-6 {
                    pixels[offset + channel] = encode(to_srgb, value);
                }
            }
        }
    }
}

/// 单通道反锐化：`c + k·(c - blur)`，夹到 `0..=1`
fn sharpen_channel(centre: f32, blur: f32, sharpen: f32) -> f32 {
    (centre + sharpen * (centre - blur)).clamp(0.0, 1.0)
}

/// 线性值 → sRGB 字节（查表 + 夹取）
fn encode(table: &[u8], linear: f32) -> u8 {
    const STEPS: f32 = 4096.0;
    let index = (linear.clamp(0.0, 1.0) * STEPS).round() as usize;
    table[index.min(4096)]
}

/// sRGB 字节 → 线性值（256 项）
fn srgb_to_linear_table() -> &'static [f32; 256] {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0f32; 256];
        for (value, slot) in table.iter_mut().enumerate() {
            *slot = srgb_to_linear(value as f32 / 255.0);
        }
        table
    })
}

/// 线性值 → sRGB 字节（4097 项，四舍五入）
fn linear_to_srgb_table() -> &'static [u8; 4097] {
    static TABLE: OnceLock<[u8; 4097]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u8; 4097];
        for (index, slot) in table.iter_mut().enumerate() {
            let linear = index as f32 / 4096.0;
            let value = linear_to_srgb(linear) * 255.0;
            *slot = value.round().clamp(0.0, 255.0) as u8;
        }
        table
    })
}

/// sRGB 分量（0..=1）→ 线性光
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// 线性光 → sRGB 分量（0..=1）
fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests;
