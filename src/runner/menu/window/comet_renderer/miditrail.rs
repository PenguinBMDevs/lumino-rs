//! MIDITrail 渲染器（轨迹拖影效果 CPU 渲染）

use lumino_midi_loader::MidiDocument;

use super::{
    collect_active_notes, collect_visible_notes, fill_bgra_black, fill_bgra_rect, is_black_key,
    note_color,
};

/// 渲染 MIDITrail 风格单帧（CPU 端简化实现）
///
/// MIDITrail 风格特点：音符拖影+3D 方块模拟+活跃键 aura 发光
pub(crate) fn render_miditrail_frame(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    document: &MidiDocument,
    tick: u32,
    ppq: u32,
    key_count: u16,
    waterfall_scroll_speed: f32,
) {
    if frame_width == 0 || frame_height == 0 || key_count == 0 {
        return;
    }

    fill_bgra_black(frame);

    let kb_height = ((frame_height as f64) * 0.12).round() as u32;
    let kb_height = kb_height.max(20).min(frame_height / 3);
    let content_height = frame_height.saturating_sub(kb_height);
    if content_height == 0 {
        return;
    }

    let ticks_per_measure = ppq * 4;
    let speed = waterfall_scroll_speed.max(0.1);
    let visible_measure_count = ((4.0f32 / speed).round()).max(1.0) as u32;
    let viewport_tick_span = (ticks_per_measure * visible_measure_count).max(1);
    let zoom_x = frame_width as f32 / key_count as f32;
    let zoom_y = content_height as f32 / viewport_tick_span as f32;

    let tick_start = tick.saturating_sub((viewport_tick_span / 3).min(10000));
    let tick_end = tick.saturating_add(viewport_tick_span * 2 / 3);

    let visible_notes = collect_visible_notes(document, tick_start, tick_end);

    // 渲染音符（拖影 + 3D 方块模拟）
    for &(track_idx, key, start_t, end_t, velocity, _length) in &visible_notes {
        let age = tick.saturating_sub(start_t);
        let total_len = end_t.saturating_sub(start_t);
        let fade_out = if total_len > 0 {
            (age as f32).min(total_len as f32) / total_len as f32
        } else {
            0.0
        };
        let trail_alpha = 1.0 - fade_out.min(0.8);
        let color = note_color(track_idx, velocity);
        let trail_color = [
            ((color[0] as f32) * trail_alpha * 0.7) as u8,
            ((color[1] as f32) * trail_alpha * 0.7) as u8,
            ((color[2] as f32) * trail_alpha * 0.7) as u8,
            (trail_alpha * 255.0) as u8,
        ];

        let note_x = (key as f32 * zoom_x).round() as u32;
        let note_w = zoom_x.ceil() as u32;
        let note_top = ((tick_end.saturating_sub(end_t)) as f32 * zoom_y).round() as u32;
        let note_h = zoom_y.round() as u32;

        // 拖影条
        let trail_h = (age as f32 * zoom_y) as u32;
        if trail_h > 0 && note_x < frame_width {
            fill_bgra_rect(
                frame,
                frame_width,
                frame_height,
                note_x,
                note_top.saturating_sub(trail_h),
                note_w,
                trail_h,
                trail_color,
            );
        }
        // 音符顶部
        fill_bgra_rect(
            frame,
            frame_width,
            frame_height,
            note_x,
            note_top,
            note_w,
            note_h.max(1),
            color,
        );
    }

    render_miditrail_keyboard(
        frame,
        frame_width,
        frame_height,
        kb_height,
        key_count,
        tick,
        document,
    );
}

fn render_miditrail_keyboard(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    kb_height: u32,
    key_count: u16,
    tick: u32,
    document: &MidiDocument,
) {
    let active_notes = collect_active_notes(document, tick);
    let mut active_keys = [false; 128];
    for &(_track, key, _vel, _start) in &active_notes {
        if key < 128 {
            active_keys[key] = true;
        }
    }

    let kb_y = frame_height.saturating_sub(kb_height);
    let white_count = (0..key_count)
        .filter(|&k| !is_black_key(k as usize))
        .count() as f32;
    let white_key_w = frame_width as f32 / white_count.max(1.0);
    let black_key_w = white_key_w * 0.65;
    let black_key_h = ((kb_height as f32) * 0.6).round() as u32;

    let mut white_idx = 0usize;
    for key in 0..key_count {
        if is_black_key(key as usize) {
            let white_before = (0..key).filter(|&k| !is_black_key(k as usize)).count();
            let x = (white_before as f32 * white_key_w) - black_key_w * 0.5;
            let color = if active_keys[key as usize] {
                fill_bgra_rect(
                    frame,
                    frame_width,
                    frame_height,
                    (x - 3.0).max(0.0) as u32,
                    kb_y.saturating_sub(3),
                    (black_key_w.ceil() + 6.0) as u32,
                    kb_height + 6,
                    [200, 180, 150, 180],
                );
                [200, 180, 150, 255]
            } else {
                [41, 41, 42, 255]
            };
            fill_bgra_rect(
                frame,
                frame_width,
                frame_height,
                x as u32,
                kb_y,
                black_key_w.ceil() as u32,
                black_key_h,
                color,
            );
        } else {
            let x = white_idx as f32 * white_key_w;
            let color = if active_keys[key as usize] {
                fill_bgra_rect(
                    frame,
                    frame_width,
                    frame_height,
                    (x - 3.0).max(0.0) as u32,
                    kb_y.saturating_sub(3),
                    (white_key_w.ceil() + 6.0) as u32,
                    kb_height + 6,
                    [255, 220, 150, 180],
                );
                [255, 220, 150, 255]
            } else {
                [235, 235, 235, 255]
            };
            fill_bgra_rect(
                frame,
                frame_width,
                frame_height,
                x as u32,
                kb_y,
                white_key_w.ceil() as u32,
                kb_height,
                color,
            );
            white_idx += 1;
        }
    }
}
