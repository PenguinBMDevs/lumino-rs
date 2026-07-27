//! Enhanced 渲染器（3D 增强风格 CPU 渲染）

use lumino_midi_loader::MidiDocument;

use super::{
    collect_active_notes, collect_visible_notes, fill_bgra_black, fill_bgra_rect, hsv_to_rgb,
    is_black_key,
};

/// 渲染 Enhanced 风格单帧（CPU 端，简化实现）
///
/// Enhanced 风格特点：音符带 HSV 色调偏移、活跃键发光效果
pub(crate) fn render_enhanced_frame(
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

    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span);

    let visible_notes = collect_visible_notes(document, tick_start, tick_end);

    // 渲染音符（HSV 色调偏移 + 发光轮廓）
    for &(track_idx, key, start_t, end_t, velocity, _length) in &visible_notes {
        let hsv_hue = (track_idx as f32 / 16.0) % 1.0;
        let note_length_ticks = end_t.saturating_sub(start_t);
        let note_progress = if note_length_ticks > 0 {
            (tick.saturating_sub(start_t)) as f32 / note_length_ticks as f32
        } else {
            0.0
        };
        let hue_shift = (hsv_hue + note_progress * 0.3) % 1.0;
        let [r, g, b] = hsv_to_rgb(hue_shift, 0.8, 0.9);

        let mut color: [u8; 4] = [b, g, r, 200];
        let vel_brightness = (velocity as f32) / 127.0;
        color[0] = ((color[0] as f32) * (0.5 + vel_brightness * 0.5)) as u8;
        color[1] = ((color[1] as f32) * (0.5 + vel_brightness * 0.5)) as u8;
        color[2] = ((color[2] as f32) * (0.5 + vel_brightness * 0.5)) as u8;

        let note_x = (key as f32 * zoom_x).round() as u32;
        let note_w = zoom_x.ceil() as u32;
        let note_top = ((tick_end.saturating_sub(end_t)) as f32 * zoom_y).round() as u32;
        let note_bottom = ((tick_end.saturating_sub(start_t)) as f32 * zoom_y).round() as u32;
        let note_h = note_bottom.saturating_sub(note_top).max(1);

        // 外发光层
        fill_bgra_rect(
            frame,
            frame_width,
            frame_height,
            note_x.saturating_sub(1),
            note_top.saturating_sub(1),
            note_w.saturating_add(2),
            note_h.saturating_add(2),
            [color[0], color[1], color[2], 60],
        );
        // 核心音符
        fill_bgra_rect(
            frame,
            frame_width,
            frame_height,
            note_x,
            note_top,
            note_w,
            note_h,
            color,
        );
    }

    // 活跃键发光
    let active_notes = collect_active_notes(document, tick);
    for &(_track, key, _velocity, _start) in &active_notes {
        if key >= key_count as usize {
            continue;
        }
        let glow_h = ((kb_height as f32) * 0.3).round() as u32;
        let key_x = (key as f32 * zoom_x).round() as u32;
        fill_bgra_rect(
            frame,
            frame_width,
            frame_height,
            key_x,
            kb_height.saturating_sub(glow_h),
            zoom_x.ceil() as u32,
            glow_h,
            [100, 200, 255, 180],
        );
    }

    render_enhanced_keyboard(
        frame,
        frame_width,
        frame_height,
        kb_height,
        key_count,
        tick,
        document,
    );
}

fn render_enhanced_keyboard(
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
