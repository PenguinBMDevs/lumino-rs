//! Miditrail 3D 渲染器实例构建

use super::types::{MiditrailInstanceGpu, MiditrailNoteGpu, MiditrailUniformGpu};

const KEYBOARD_HEIGHT: f32 = 0.05;
const NOTE_HEIGHT: f32 = 0.05;
const NOTE_Y: f32 = 0.06;
const BLACK_KEY_ELEVATION: f32 = 0.02;
const BLACK_KEY_HEIGHT: f32 = 0.04;
const SCENE_DEPTH: f32 = 1.0;
const BLACK_KEY_WIDTH_RATIO: f32 = 0.58;

/// 更新键位布局缓存。
pub fn update_key_positions(
    key_count: u32,
    last_key_count: &mut u32,
    key_positions: &mut Vec<f32>,
    key_widths: &mut Vec<f32>,
) {
    if key_count == 0 || key_count == *last_key_count {
        return;
    }
    *last_key_count = key_count;
    key_positions.resize(key_count as usize, 0.0);
    key_widths.resize(key_count as usize, 0.0);

    let count = key_count as usize;
    let num_white = (0..count)
        .filter(|k| !is_black_key(*k as u32))
        .count()
        .max(1);
    let white_width = 1.0 / num_white as f32;
    let black_width = white_width * BLACK_KEY_WIDTH_RATIO;

    let mut pos = 0.0f32;
    for i in 0..count {
        if is_black_key(i as u32) {
            key_positions[i] = pos - black_width * 0.5;
            key_widths[i] = black_width;
        } else {
            key_positions[i] = pos;
            key_widths[i] = white_width;
            pos += white_width;
        }
    }
}

/// 判断 MIDI 键是否为黑键。
#[must_use]
pub fn is_black_key(key: u32) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

/// 将 [r, g, b, a] 颜色打包为 `0xRRGGBBAA`。
#[must_use]
pub fn pack_color(color: [f32; 4]) -> u32 {
    let r = (color[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (color[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (color[2].clamp(0.0, 1.0) * 255.0) as u32;
    let a = (color[3].clamp(0.0, 1.0) * 255.0) as u32;
    (r << 24) | (g << 16) | (b << 8) | a
}

/// 构建可见音符的实例数据。
pub fn build_note_instances(
    uniform: &MiditrailUniformGpu,
    notes: &[MiditrailNoteGpu],
    key_positions: &[f32],
    key_widths: &[f32],
    out: &mut Vec<MiditrailInstanceGpu>,
) {
    let tick = uniform.tick;
    let ppq = uniform.ppq.max(1);
    let speed = uniform.speed.max(0.1);
    let ticks_per_measure = ppq * 4;
    let visible_measure_count = ((4.0 / speed).round()).max(1.0) as u32;
    let viewport_tick_span = (ticks_per_measure * visible_measure_count).max(1) as f32;
    let scene_depth = SCENE_DEPTH;
    let note_height = NOTE_HEIGHT;
    let note_y = NOTE_Y;

    for note in notes {
        if !note.is_visible_at(tick) {
            continue;
        }
        let key = note.key as usize;
        if key >= key_positions.len() {
            continue;
        }
        let left = key_positions[key];
        let width = key_widths[key];
        let _x = left + width * 0.5;

        let visible_start = note.start_tick.max(tick);
        let visible_end = note.end_tick;
        let z_start =
            -((visible_start.saturating_sub(tick)) as f32 / viewport_tick_span * scene_depth);
        let z_end = -((visible_end.saturating_sub(tick)) as f32 / viewport_tick_span * scene_depth);
        if z_end >= z_start {
            continue;
        }
        let z_center = (z_start + z_end) * 0.5;
        let z_length = z_start - z_end;

        let scale = [width * 0.92, note_height, z_length];
        let translation = [left + width * 0.04, note_y, z_center - z_length * 0.5];

        out.push(MiditrailInstanceGpu::new(
            translation,
            scale,
            note.color_packed,
            false,
        ));
    }
}

/// 构建琴键实例。
pub fn build_key_instances(
    uniform: &MiditrailUniformGpu,
    notes: &[MiditrailNoteGpu],
    key_positions: &[f32],
    key_widths: &[f32],
    out: &mut Vec<MiditrailInstanceGpu>,
) {
    let tick = uniform.tick;
    let key_count = uniform.key_count as usize;
    let key_count = key_count.min(key_positions.len());

    // 先找出每个键的激活颜色（有音符时覆盖默认颜色）
    let mut active_colors = [None; 128];
    for note in notes {
        if !note.is_active_at(tick) {
            continue;
        }
        let key = note.key as usize;
        if key < 128 {
            active_colors[key] = Some(note.color_packed);
        }
    }

    for i in 0..key_count {
        let left = key_positions[i];
        let width = key_widths[i];
        let is_black = is_black_key(i as u32);
        let (y, height) = if is_black {
            (BLACK_KEY_ELEVATION, BLACK_KEY_HEIGHT)
        } else {
            (0.0, KEYBOARD_HEIGHT)
        };
        let color = active_colors[i].unwrap_or_else(|| {
            if is_black {
                pack_color([0.12, 0.12, 0.12, 1.0])
            } else {
                pack_color([0.92, 0.92, 0.92, 1.0])
            }
        });
        let scale = [width, height, 0.02];
        let translation = [left, y, -0.01];
        out.push(MiditrailInstanceGpu::new(translation, scale, color, true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_black_keys() {
        assert!(is_black_key(1));
        assert!(is_black_key(61)); // C#4 + 5 octaves
        assert!(!is_black_key(0));
        assert!(!is_black_key(60));
    }

    #[test]
    fn test_key_positions() {
        let mut positions = Vec::new();
        let mut widths = Vec::new();
        let mut last = 0u32;
        update_key_positions(128, &mut last, &mut positions, &mut widths);
        assert_eq!(positions.len(), 128);
        assert_eq!(widths.len(), 128);
        // 白键总宽度应约为 1.0
        let white_total: f32 = positions
            .iter()
            .enumerate()
            .filter(|(i, _)| !is_black_key(*i as u32))
            .map(|(i, _)| widths[i])
            .sum();
        assert!((white_total - 1.0).abs() < 1e-5);
        // 黑键应比相邻白键窄
        assert!(widths[1] < widths[0]);
    }

    #[test]
    fn test_build_instances() {
        let mut positions = Vec::new();
        let mut widths = Vec::new();
        let mut last = 0u32;
        update_key_positions(128, &mut last, &mut positions, &mut widths);

        let uniform = MiditrailUniformGpu::default();
        let notes = vec![MiditrailNoteGpu {
            key: 60,
            start_tick: 0,
            end_tick: 1000,
            color_packed: 0xFFFF0000,
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        }];
        let mut out = Vec::new();
        build_note_instances(&uniform, &notes, &positions, &widths, &mut out);
        build_key_instances(&uniform, &notes, &positions, &widths, &mut out);
        // 128 个键 + 1 个音符
        assert_eq!(out.len(), 129);
    }
}
