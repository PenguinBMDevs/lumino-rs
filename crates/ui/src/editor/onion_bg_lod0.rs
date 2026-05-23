//! 洋葱皮背景 LOD0（最精细层级）瓦片像素生成与 GPU 上传
//!
//! 将 MIDI document 中除当前音轨外的所有音轨音符，
//! 光栅化为 100 tick/像素 的 RGBA 像素格，上传到瓦片池纹理。

use iced_wgpu::wgpu;
use std::sync::Arc;

use super::onion_bg_pool::OnionBgTilePool;
use lumino_core::midi::MidiDocument;

/// LOD0 像素数据
pub struct Lod0PixelData {
    /// RGBA 像素缓冲区（width × height × 4 bytes）
    pub pixels: Vec<u8>,
    /// 纹理宽度（像素）
    pub width: u32,
    /// 纹理高度（像素）
    pub height: u32,
    /// 参与绘制的音符总数
    pub note_count: usize,
}

/// 生成 LOD0 瓦片像素数据
///
/// 将视口内所有非当前音轨的音符光栅化为 100 tick/像素 的 RGBA 位图。
/// 像素颜色：半透明灰蓝 (R=100, G=150, B=200, A=80)。
///
/// # 参数
/// - `document`: MIDI 文档引用
/// - `current_track`: 当前正在编辑的音轨（跳过）
/// - `tick_start / tick_end`: tick 视口范围
/// - `key_min / key_max`: key 视口范围
pub fn generate_lod0_pixels(
    document: Option<&Arc<MidiDocument>>,
    current_track: usize,
    tick_start: f32,
    tick_end: f32,
    key_min: u16,
    key_max: u16,
) -> Lod0PixelData {
    puffin::profile_function!();
    let Some(doc) = document else {
        return Lod0PixelData {
            pixels: Vec::new(),
            width: 0,
            height: 0,
            note_count: 0,
        };
    };

    if tick_end <= tick_start || key_max < key_min {
        return Lod0PixelData {
            pixels: Vec::new(),
            width: 0,
            height: 0,
            note_count: 0,
        };
    }

    const PIXELS_PER_TICK: f32 = 0.5; // 2 ticks → 1 px
    const PIXELS_PER_KEY: f32 = 32.0; // 1 key → 32 px
    const TILE_HEIGHT: u32 = 512; // 匹配池纹理高度 (16 keys × 32 px)

    let span_ticks = tick_end - tick_start;
    let width = (span_ticks * PIXELS_PER_TICK).max(1.0) as u32;
    let height = TILE_HEIGHT;

    // 初始化像素缓冲区
    let pixel_count = (width as usize) * (height as usize);
    let mut pixels = vec![0u8; pixel_count * 4];
    // 一次性查询所有非当前音轨的音符，避免多次二分查找和 Vec 分配
    let all_notes = {
        puffin::profile_scope!("get_all_notes_in_range_except");
        doc.get_all_notes_in_range_except(current_track, tick_start, tick_end)
    };
    let note_count = all_notes.len();

    for &(ntick, nkey, nlength, _vel, _ch) in &all_notes {
        let nkey = nkey as u16;
        if nkey < key_min || nkey > key_max {
            continue;
        }

        let note_end = ntick + nlength;
        if note_end < tick_start || ntick > tick_end {
            continue;
        }

        // 计算像素列范围（PIXELS_PER_TICK = 0.5）
        let px_start = ((ntick - tick_start) * PIXELS_PER_TICK).max(0.0) as u32;
        let px_end = ((note_end - tick_start) * PIXELS_PER_TICK)
            .min(span_ticks * PIXELS_PER_TICK)
            .max(0.0) as u32;

        let y = ((nkey - key_min) as f32 * PIXELS_PER_KEY) as u32;
        if y >= height {
            continue;
        }

        // 填充音符跨越的像素行（高度为 PIXELS_PER_KEY 像素）
        let y_end = (y + PIXELS_PER_KEY as u32).min(height);
        {
            let px_end_clamped = px_end.min(width.saturating_sub(1));
            if px_start <= px_end_clamped {
                // 预计算颜色为 u32（小端序：RGBA → 0xFF3264FF）
                const COLOR_U32: u32 = 0xFF_32_64_FF; // A=FF, B=64, G=32, R=FF (little endian)
                let row_fill_px = (px_end_clamped - px_start + 1) as usize;
                for py in y..y_end {
                    let row_start = (py * width + px_start) as usize;
                    let row_end_px = row_start + row_fill_px;
                    if row_end_px <= pixel_count {
                        // 按 u32 批量写入，一次写入 4 字节（1 个像素）
                        unsafe {
                            let ptr = pixels.as_mut_ptr() as *mut u32;
                            for px in 0..row_fill_px {
                                ptr.add(row_start + px).write_volatile(COLOR_U32);
                            }
                        }
                    }
                }
            }
        }
    } // note loop

    Lod0PixelData {
        pixels,
        width,
        height,
        note_count,
    }
}

/// 将 LOD0 像素数据上传到瓦片池指定索引的纹理
///
/// 委托给池的 `upload_texture`，使用 `create_texture_with_data` 确保布局转换。
pub fn upload_lod0_to_gpu(data: &Lod0PixelData, pool_index: u16, pool: &mut OnionBgTilePool) {
    puffin::profile_function!();
    if data.pixels.is_empty() || data.width == 0 || data.height == 0 {
        return;
    }

    let raw_stride = data.width * 4;
    let aligned_stride = (raw_stride + wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1)
        & !(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1);

    let (pixels, _bytes_per_row) = if raw_stride == aligned_stride {
        (data.pixels.clone(), raw_stride)
    } else {
        let mut padded = data.pixels.clone();
        padded.resize(aligned_stride as usize * data.height as usize, 0);
        (padded, aligned_stride)
    };

    pool.upload_texture(pool_index, &pixels, data.width, data.height);
}
