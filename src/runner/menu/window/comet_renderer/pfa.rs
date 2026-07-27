//! PFA 渲染器（侧视图钢琴卷帘 CPU 渲染）

use lumino_midi_loader::MidiDocument;

use super::{collect_visible_notes, fill_bgra_black, fill_bgra_rect, is_black_key, note_color};

/// 渲染 PFA 风格单帧（CPU 端简化实现）
///
/// PFA 风格特点：侧视图钢琴卷帘+力度暗化边框+纯过程化绘制
pub(crate) fn render_pfa_frame(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    document: &MidiDocument,
    tick: u32,
    ppq: u32,
    key_count: u16,
    _velocity_panel_rect: Option<(f32, f32, f32, f32)>,
) {
    if frame_width == 0 || frame_height == 0 || key_count == 0 {
        return;
    }

    fill_bgra_black(frame);

    let keyboard_width = 60.0f32;
    let content_width = frame_width.saturating_sub(keyboard_width as u32);
    if content_width == 0 {
        return;
    }

    let viewport_tick_span = (ppq * 16).max(1);
    let zoom_x = content_width as f32 / viewport_tick_span as f32;
    let zoom_y = frame_height as f32 / viewport_tick_span as f32;

    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span);

    // 渲染 PFA 风格键盘
    let white_count = (0..key_count)
        .filter(|&k| !is_black_key(k as usize))
        .count() as f32;
    let white_key_w = keyboard_width / white_count.max(1.0);
    let black_key_w = white_key_w * 0.65;

    let mut white_idx = 0usize;
    for key in 0..key_count {
        if is_black_key(key as usize) {
            let white_before = (0..key).filter(|&k| !is_black_key(k as usize)).count();
            let x = (white_before as f32 * white_key_w) - black_key_w * 0.5;
            fill_bgra_rect(
                frame,
                frame_width,
                frame_height,
                x as u32,
                0,
                black_key_w.ceil() as u32,
                frame_height,
                [41, 41, 42, 255],
            );
        } else {
            let x = white_idx as f32 * white_key_w;
            fill_bgra_rect(
                frame,
                frame_width,
                frame_height,
                x as u32,
                0,
                white_key_w.ceil() as u32,
                frame_height,
                [220, 220, 220, 255],
            );
            white_idx += 1;
        }
    }

    // 渲染音符
    let visible_notes = collect_visible_notes(document, tick_start, tick_end);
    for &(track_idx, key, start_t, end_t, velocity, _length) in &visible_notes {
        let color = note_color(track_idx, velocity);
        let note_x = keyboard_width + ((tick_start as f32 - start_t as f32) * zoom_x).max(0.0);
        let note_w = ((end_t - start_t) as f32 * zoom_x).max(2.0);
        let key_y = frame_height as f32 * (1.0 - (key as f32 + 1.0) / key_count as f32);
        let note_h = zoom_y.max(1.0);

        if note_w > 0.0 {
            fill_bgra_rect(
                frame,
                frame_width,
                frame_height,
                note_x as u32,
                key_y as u32,
                note_w as u32,
                note_h as u32,
                color,
            );
        }
    }
}
