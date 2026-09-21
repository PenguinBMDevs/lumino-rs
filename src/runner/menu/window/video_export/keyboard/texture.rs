use lumino_gfx::is_black_key;

use super::*;

/// 生成完整键盘贴图（BGRA 像素数据，与视频帧格式一致）
///
/// 生成一个从最高键到最低键的完整键盘图像，与 note shader 的 Y 轴方向一致
/// （高键在上，低键在下）。返回 (pixels, width, height)。
///
/// 注意：key_count 固定为 128 以匹配标准 MIDI 键盘。
/// 使用 ceil() 进行像素到键位的映射，确保键盘边界与 note shader 对齐。
/// 直接生成 BGRA 格式以避免每帧合成时做 RGBA→BGRA 转换。
pub fn generate_keyboard_texture(_width: u32, height: u32, key_count: u16) -> (Vec<u8>, u32, u32) {
    const KB_WIDTH: f32 = 60.0;
    const RULER_HEIGHT: f32 = 30.0;
    const KEY_COUNT: u16 = 128;
    let kb_w = KB_WIDTH as u32;

    // 键盘区域从 ruler 下方开始
    let ruler_h = RULER_HEIGHT as u32;
    if height <= ruler_h || key_count == 0 {
        return (Vec::new(), 0, 0);
    }
    let kb_h = height - ruler_h;
    let key_count_f = KEY_COUNT as f32;
    let zoom_y = kb_h as f32 / key_count_f;

    let mut pixels = vec![0u8; (kb_w * kb_h * 4) as usize];

    for py in 0..kb_h {
        // Y 向：键 0 在底部，最高键在顶部（与 note shader 一致）
        // 使用 ceil() 确保键盘边界与 note shader 的精确边界匹配：
        // note shader: screen_y = (max_key_index - key) * zoom_y + ruler_height
        // 键盘像素 py 映射到 key = ceil(max_key_index - py / zoom_y)
        // 这保证了每个键占据的像素范围与 note shader 渲染的矩形完全一致
        let key_f = (key_count_f - 1.0) - py as f32 / zoom_y;
        let key_idx = key_f.ceil() as i32;
        if key_idx < 0 || key_idx >= KEY_COUNT as i32 {
            continue;
        }
        let is_black = is_black_key(key_idx as isize);

        // 标准黑白色：白键纯白，黑键纯黑
        // 白键底部添加 1px 浅灰边框作键位分隔
        let is_bottom_border = {
            let next_key_f = (key_count_f - 1.0) - (py as f32 + 1.0) / zoom_y;
            let next_key_idx = next_key_f.ceil() as i32;
            next_key_idx != key_idx
        };

        for px in 0..kb_w {
            let idx = ((py * kb_w + px) * 4) as usize;

            let (r, g, b) = if is_black {
                // 黑键：纯黑，底部加 1px 深灰边框
                if is_bottom_border {
                    (40, 40, 40)
                } else {
                    (0, 0, 0) // 标准黑键
                }
            } else {
                // 白键：纯白，底部加 1px 浅灰边框
                if is_bottom_border {
                    (200, 200, 200)
                } else {
                    (255, 255, 255) // 标准白键
                }
            };

            pixels[idx] = b; // BGRA: B 通道
            pixels[idx + 1] = g; // BGRA: G 通道
            pixels[idx + 2] = r; // BGRA: R 通道
            pixels[idx + 3] = 255;
        }
    }

    (pixels, kb_w, kb_h)
}

/// 将键盘贴图合成到视频帧上（BGRA 格式，in-place 修改），并叠加演奏高亮颜色
///
/// 贴图与帧均为 BGRA 格式，无高亮时直接逐行 memcpy；有高亮时按 60% 不透明度
/// 叠加对应音轨颜色，与编辑器左侧键盘的洋葱皮效果保持一致。
///
/// # 性能说明
/// 非高亮行走 `copy_from_slice` 快速路径（逐行 memcpy）。
/// 高亮行走标量 blend 循环，通过预计算权重系数避免每像素重复除法。
pub fn composite_keyboard(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    keyboard_pixels: &[u8],
    kb_width: u32,
    kb_height: u32,
    key_colors: &[u8; KEY_COLOR_BYTES],
) {
    const RULER_HEIGHT: u32 = 30;
    if frame_width == 0 || frame_height == 0 || keyboard_pixels.is_empty() {
        return;
    }
    let kb_w = kb_width.min(frame_width);
    let kb_h = kb_height.min(frame_height.saturating_sub(RULER_HEIGHT));
    if kb_w == 0 || kb_h == 0 {
        return;
    }
    let row_bytes = (kb_w * 4) as usize;
    let frame_stride = (frame_width * 4) as usize;
    let kb_stride = (kb_width * 4) as usize;
    let key_count_f = EXPORT_KEY_COUNT as f32;
    let zoom_y = kb_h as f32 / key_count_f;

    // 预计算每像素所需颜色分量（按 key_idx 索引）
    // 结构: (overlay_b, overlay_g, overlay_r, overlay_alpha)
    // key_colors 为 RGBA 格式，frame 为 BGRA，读取时交换 R↔B
    let mut per_key_overlay = [(0i32, 0i32, 0i32, 0u8); 128];
    for (key_idx, colors) in key_colors.as_chunks::<4>().0.iter().enumerate().take(128) {
        let alpha = colors[3];
        if alpha != 0 {
            let scaled_alpha = (alpha as u16 * OVERLAY_ALPHA as u16 / 255) as u8;
            per_key_overlay[key_idx] = (
                colors[2] as i32, // B (key_colors RGBA → overlay_b)
                colors[1] as i32, // G
                colors[0] as i32, // R (key_colors RGBA → overlay_r)
                scaled_alpha,
            );
        }
    }

    for py in 0..kb_h {
        let frame_y = RULER_HEIGHT + py;
        if frame_y >= frame_height {
            break;
        }
        let frame_start = frame_y as usize * frame_stride;
        let kb_start = py as usize * kb_stride;
        if frame_start + row_bytes > frame.len() || kb_start + row_bytes > keyboard_pixels.len() {
            continue;
        }
        let frame_row_end = frame_start + row_bytes;

        let key_f = (key_count_f - 1.0) - py as f32 / zoom_y;
        let key_idx = key_f.ceil() as i32;
        if key_idx < 0 || key_idx >= EXPORT_KEY_COUNT as i32 {
            frame[frame_start..frame_row_end]
                .copy_from_slice(&keyboard_pixels[kb_start..kb_start + row_bytes]);
            continue;
        }

        let (ob, og, or_, overlay_alpha) = per_key_overlay[key_idx as usize];
        if overlay_alpha == 0 {
            frame[frame_start..frame_row_end]
                .copy_from_slice(&keyboard_pixels[kb_start..kb_start + row_bytes]);
            continue;
        }

        // 高亮行：预计算权重，避免每像素除法
        let alpha_i = overlay_alpha as i32;
        // blend = (base * (255 - alpha) + overlay * alpha) / 255
        // 展开为: base + (overlay - base) * alpha / 255
        let frame_row = &mut frame[frame_start..frame_row_end];
        let kb_row = &keyboard_pixels[kb_start..kb_start + row_bytes];

        // 使用 as_chunks 自动向量化友好的方式处理每像素 4 字节
        for (fchunk, kchunk) in frame_row
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(kb_row.as_chunks::<4>().0.iter())
        {
            let blue_ch = kchunk[0] as i32;
            let green_ch = kchunk[1] as i32;
            let red_ch = kchunk[2] as i32;
            fchunk[0] = (blue_ch + (ob - blue_ch) * alpha_i / 255).clamp(0, 255) as u8;
            fchunk[1] = (green_ch + (og - green_ch) * alpha_i / 255).clamp(0, 255) as u8;
            fchunk[2] = (red_ch + (or_ - red_ch) * alpha_i / 255).clamp(0, 255) as u8;
            fchunk[3] = 255;
        }
    }
}
