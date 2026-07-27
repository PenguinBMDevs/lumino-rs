//! Channels 渲染器（通道热力图 CPU 渲染）

use lumino_midi_loader::MidiDocument;

use super::{fill_bgra_black, fill_bgra_rect, hsv_to_rgb};

/// 渲染 Channels 风格单帧（通道热力图）
pub(crate) fn render_channels_frame(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    document: &MidiDocument,
    tick: u32,
    key_count: u16,
    channel_count: u16,
) {
    if frame_width == 0 || frame_height == 0 || key_count == 0 || channel_count == 0 {
        return;
    }

    fill_bgra_black(frame);

    let kb_width = ((frame_width as f32) * 0.12).round() as u32;
    let content_width = frame_width.saturating_sub(kb_width);
    if content_width == 0 {
        return;
    }

    let cell_w = content_width as f32 / key_count as f32;
    let cell_h = frame_height as f32 / channel_count as f32;

    // 构建活跃度网格
    let mut channel_grid = [[0u8; 16]; 128];
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        for n in track_notes {
            if n.start_tick <= tick && n.end_tick > tick {
                let key = n.key as usize;
                let channel = track_idx.min(15);
                if key < 128 && channel < 16 {
                    channel_grid[key][channel] = 1;
                }
            }
        }
    }

    // 渲染热力图
    for (key, row) in channel_grid.iter().enumerate().take(key_count as usize) {
        for (channel, &value) in row.iter().enumerate().take(channel_count as usize) {
            if value == 0 {
                continue;
            }
            let x = (key as f32 * cell_w) as u32;
            let y = frame_height.saturating_sub(((channel + 1) as f32 * cell_h) as u32);
            let w = cell_w.ceil() as u32;
            let h = cell_h.ceil() as u32;
            let hue = channel as f32 / channel_count as f32;
            let [r, g, b] = hsv_to_rgb(hue, 0.7, 0.9);
            fill_bgra_rect(frame, frame_width, frame_height, x, y, w, h, [b, g, r, 255]);
        }
    }
}
