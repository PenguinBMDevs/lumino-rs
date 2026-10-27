//! 编辑覆盖层：选择高亮、框选矩形、拖动 ghost。
//!
//! 覆盖层直接写在已完成的光栅帧上，不参与 band 渲染；拖动时先恢复 drag 起始
//! 帧，再叠 ghost，所以每次鼠标移动不需要重扫 1.6 亿音符。

use std::collections::BTreeSet;

use lumino_midi_loader::{MidiDocument, NoteEvent};

use crate::{
    camera::Camera,
    edit::Drag,
    render::color::{NoteStyle, blend_overlay_fast, blend_preview_fast},
};

const SELECTION_COLOR: u32 = 0xEA_F6_FF;
const BOX_COLOR: u32 = 0x9C_C8_FF;

/// 音符在屏幕上的矩形（含左右/上下裁剪）。
fn note_rect(
    camera: &Camera,
    note: &NoteEvent,
    delta_tick: i32,
    delta_key: i32,
) -> Option<(i32, i32, i32, i32)> {
    let start = i64::from(note.start_tick) + i64::from(delta_tick);
    let end = i64::from(note.end_tick) + i64::from(delta_tick);
    let key = (i32::from(note.key) + delta_key).clamp(0, 127) as u16;
    let x0 = camera.tick_to_x(start.max(0) as f32).round() as i32;
    let x1 = camera.tick_to_x(end.max(0) as f32).round() as i32;
    let y0 = camera.key_to_y(key).round() as i32;
    let y1 = y0 + camera.zoom_y.round().max(1.0) as i32;
    Some((x0.min(x1), y0, x0.max(x1).max(x0.min(x1) + 1), y1))
}

fn clip_rect(camera: &Camera, rect: (i32, i32, i32, i32)) -> Option<(usize, usize, usize, usize)> {
    let left = (camera.keyboard_width.round().max(0.0) as i32).max(2);
    let top = (camera.ruler_height.round().max(0.0) as i32).max(1);
    let x0 = rect.0.max(left);
    let y0 = rect.1.max(top);
    let x1 = rect.2.min(camera.width as i32);
    let y1 = rect.3.min(camera.height as i32);
    (x1 > x0 && y1 > y0).then_some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
}

fn fill_rect(
    frame: &mut [u32],
    stride: usize,
    rect: (usize, usize, usize, usize),
    color: u32,
    weight: u32,
) {
    for y in rect.1..rect.3 {
        let row = &mut frame[y * stride..(y + 1) * stride];
        for pixel in &mut row[rect.0..rect.2] {
            *pixel = blend_overlay_fast(*pixel, color, weight);
        }
    }
}

/// 画选中音符的亮色描边。
pub fn draw_selection(
    frame: &mut [u32],
    stride: usize,
    camera: &Camera,
    document: &MidiDocument,
    track: usize,
    selection: &BTreeSet<usize>,
) {
    if selection.is_empty() {
        return;
    }
    let notes = document.track_notes(track);
    for &position in selection.iter().take(20_000) {
        let Some(note) = notes.get(position) else {
            continue;
        };
        let Some(rect) = note_rect(camera, note, 0, 0) else {
            continue;
        };
        let Some((x0, y0, x1, y1)) = clip_rect(camera, rect) else {
            continue;
        };
        let thickness = 2.min((x1 - x0).min(y1 - y0)).max(1);
        let row_color = SELECTION_COLOR;
        for y in y0..y1 {
            let row = &mut frame[y * stride..(y + 1) * stride];
            if y < y0 + thickness || y + thickness >= y1 {
                row[x0..x1].fill(row_color);
            } else {
                row[x0..(x0 + thickness).min(x1)].fill(row_color);
                row[x1.saturating_sub(thickness).max(x0)..x1].fill(row_color);
            }
        }
    }
}

/// 画拖动中的 ghost / 框选矩形。
pub fn draw_drag(
    frame: &mut [u32],
    stride: usize,
    camera: &Camera,
    drag: &Drag,
    style: &NoteStyle,
    track: usize,
) {
    match drag {
        Drag::Box {
            start_tick,
            start_key,
            current_tick,
            current_key,
        } => {
            let x0 = camera.tick_to_x(*start_tick as f32).round() as i32;
            let x1 = camera.tick_to_x(*current_tick as f32).round() as i32;
            let y0 = camera.key_to_y(*start_key as u16).round() as i32;
            let y1 = camera
                .key_to_y(*current_key as u16)
                .round()
                .max(y0 as f32 + camera.zoom_y) as i32;
            let rect = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
            if let Some((x0, y0, x1, y1)) = clip_rect(camera, rect) {
                fill_rect(frame, stride, (x0, y0, x1, y1), BOX_COLOR, 2);
                let border = 2;
                for y in y0..y1.min(y0 + border) {
                    frame[y * stride + x0..y * stride + x1].fill(BOX_COLOR);
                }
                for y in y1.saturating_sub(border)..y1 {
                    frame[y * stride + x0..y * stride + x1].fill(BOX_COLOR);
                }
            }
        }
        Drag::Move {
            delta_tick,
            delta_key,
            notes,
            ..
        } => {
            let color = style.fill(track, true);
            for note in notes.iter().take(20_000) {
                let Some(rect) = note_rect(camera, note, *delta_tick, *delta_key) else {
                    continue;
                };
                if let Some((x0, y0, x1, y1)) = clip_rect(camera, rect) {
                    for y in y0..y1 {
                        let row = &mut frame[y * stride..(y + 1) * stride];
                        for pixel in &mut row[x0..x1] {
                            *pixel = blend_preview_fast(*pixel, color);
                        }
                    }
                }
            }
        }
    }
}
