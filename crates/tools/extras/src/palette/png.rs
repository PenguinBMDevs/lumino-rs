//! 调色板 PNG 解码
//!
//! 支持 RGBA、RGB、Indexed 三种 PNG 颜色类型，返回所有像素的 RGBA 颜色列表。

use png::ColorType;

use super::PaletteColor;

/// PNG 解压错误
#[derive(Debug)]
pub struct PngDecodeError {
    msg: String,
}

impl std::fmt::Display for PngDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PNG 解码错误: {}", self.msg)
    }
}

impl std::error::Error for PngDecodeError {}

/// 解码调色板 PNG 图片
///
/// 支持的格式：
/// - 颜色类型：RGB (2)、RGBA (6)、Indexed (3)
/// - 宽度：任意（推荐 16 或 32 像素）
/// - 高度：任意（推荐 1 或 8 行）
///
/// 返回所有像素的 RGBA 颜色列表。
pub(crate) fn decode_palette_png(data: &[u8]) -> Result<Vec<PaletteColor>, PngDecodeError> {
    let decoder = png::Decoder::new(data);
    let mut reader = decoder.read_info().map_err(|e| PngDecodeError {
        msg: format!("无法读取 PNG: {}", e),
    })?;

    // Clone the info data to avoid borrow issues with reader
    let width = reader.info().width as usize;
    let height = reader.info().height as usize;
    let color_type = reader.info().color_type;
    let bit_depth = reader.info().bit_depth;
    let palette_data = reader.info().palette.clone();
    let trns_data = reader.info().trns.clone();

    // 分配输出缓冲
    let output_size = reader.output_buffer_size();
    let mut buf = vec![0u8; output_size];
    let _ = reader.next_frame(&mut buf).map_err(|e| PngDecodeError {
        msg: format!("无法解码帧: {}", e),
    })?;

    let colors = match (color_type, bit_depth) {
        (ColorType::Rgba, png::BitDepth::Eight) => {
            // RGBA: 每像素 4 字节
            let pixel_count = width * height;
            let mut colors = Vec::with_capacity(pixel_count);
            for chunk in buf.as_chunks::<4>().0 {
                colors.push([chunk[0], chunk[1], chunk[2], chunk[3]]);
            }
            colors
        }
        (ColorType::Rgb, png::BitDepth::Eight) => {
            // RGB: 每像素 3 字节，alpha 设为 255
            let pixel_count = width * height;
            let mut colors = Vec::with_capacity(pixel_count);
            for chunk in buf.as_chunks::<3>().0 {
                colors.push([chunk[0], chunk[1], chunk[2], 255]);
            }
            colors
        }
        (ColorType::Indexed, _) => {
            // Indexed: 需要从 PLTE 和 tRNS 块重建颜色
            let pixel_count = width * height;
            let palette = palette_data.as_ref().ok_or_else(|| PngDecodeError {
                msg: "索引色 PNG 缺少 PLTE 块".to_string(),
            })?;

            let mut colors = Vec::with_capacity(pixel_count);
            for &palette_idx_byte in buf.iter().take(pixel_count) {
                let palette_idx = palette_idx_byte as usize;
                if palette_idx * 3 + 2 < palette.len() {
                    let red = palette[palette_idx * 3];
                    let green = palette[palette_idx * 3 + 1];
                    let blue = palette[palette_idx * 3 + 2];
                    let alpha = trns_data
                        .as_ref()
                        .and_then(|t| t.get(palette_idx))
                        .copied()
                        .unwrap_or(255);
                    colors.push([red, green, blue, alpha]);
                } else {
                    colors.push([0, 0, 0, 255]);
                }
            }
            colors
        }
        _ => {
            return Err(PngDecodeError {
                msg: format!(
                    "不支持的 PNG 格式: color_type={:?}, bit_depth={:?}",
                    color_type, bit_depth
                ),
            });
        }
    };

    if colors.is_empty() {
        return Err(PngDecodeError {
            msg: "调色板中没有颜色".to_string(),
        });
    }

    tracing::debug!(
        "[Palette] 加载: {}x{}, {} 种颜色",
        width,
        height,
        colors.len()
    );

    Ok(colors)
}
