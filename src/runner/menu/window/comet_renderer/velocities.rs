//! Velocities 渲染器（力度热力图 CPU 渲染）

use lumino_midi_loader::MidiDocument;

use super::{collect_active_notes, fill_bgra_black, fill_bgra_rect};

/// 渲染 Velocities 风格单帧（力度热力图）
pub(crate) fn render_velocities_frame(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    document: &MidiDocument,
    tick: u32,
    key_count: u16,
) {
    if frame_width == 0 || frame_height == 0 || key_count == 0 {
        return;
    }

    fill_bgra_black(frame);

    let kb_width = ((frame_width as f32) * 0.15).round() as u32;
    let content_width = frame_width.saturating_sub(kb_width);
    if content_width == 0 {
        return;
    }

    let cell_w = content_width as f32 / key_count as f32;
    let cell_h = frame_height as f32 / 128.0;

    // 绘制热力图网格
    for channel in 0..16 {
        for key in 0..key_count as usize {
            let x = (key as f32 * cell_w) as u32;
            let y = frame_height.saturating_sub(((channel + 1) as f32 * cell_h) as u32);
            let w = cell_w.ceil() as u32;
            let h = cell_h.ceil() as u32;
            fill_bgra_rect(
                frame,
                frame_width,
                frame_height,
                x,
                y,
                w,
                h,
                [40, 40, 48, 255],
            );
        }
    }

    // 绘制活跃音符
    let active_notes = collect_active_notes(document, tick);
    for &(_track, key, velocity, _start) in &active_notes {
        if key >= key_count as usize {
            continue;
        }
        let x = (key as f32 * cell_w) as u32;
        let y = frame_height.saturating_sub(((velocity as f32 + 1.0) * cell_h) as u32);
        let w = cell_w.ceil() as u32;
        let h = cell_h.ceil() as u32;
        let vel_f = (velocity as f32) / 127.0;
        let brightness = 0.3 + vel_f * 0.7;
        fill_bgra_rect(
            frame,
            frame_width,
            frame_height,
            x,
            y,
            w,
            h,
            [
                (50.0 * brightness) as u8,
                (180.0 * brightness) as u8,
                (255.0 * brightness) as u8,
                255,
            ],
        );
    }
}
