//! Miditrail 3D 渲染器实例构建

use super::MIDITRAIL_SCENE_DEPTH;
use super::types::{
    MiditrailAuraInstanceGpu, MiditrailInstanceGpu, MiditrailNoteGpu, MiditrailUniformGpu,
};

const KEYBOARD_HEIGHT: f32 = 0.012;
const WHITE_KEY_DEPTH: f32 = 0.07;
const BLACK_KEY_DEPTH: f32 = 0.0448;
const NOTE_HEIGHT: f32 = 0.007;
const NOTE_Y: f32 = 0.0005;
const NOTE_Z_OFFSET: f32 = 0.012;
const BLACK_KEY_ELEVATION: f32 = 0.0;
const BLACK_KEY_HEIGHT: f32 = 0.024;
const BLACK_KEY_WIDTH_RATIO: f32 = 0.58;

/// 当前 tick 下被按下的键信息（同一键多个音符时取最后一个音符颜色）。
#[derive(Debug, Clone, Copy)]
pub struct ActiveKeys {
    /// 每个键是否被按下。
    pub pressed: [bool; 128],
    /// 每个键的激活颜色。
    pub colors: [u32; 128],
}

/// 计算当前 tick 下每个键是否被按下及其对应颜色。
///
/// 同一键多个音符激活时，取 `notes` 中最后一个音符的颜色，与
/// `build_key_instances` / `build_aura_instances` 历史行为一致。
#[must_use]
pub fn compute_active_keys(tick: u32, notes: &[MiditrailNoteGpu]) -> ActiveKeys {
    let mut pressed = [false; 128];
    let mut colors = [0u32; 128];
    for note in notes {
        if note.is_active_at(tick) {
            let key = note.key as usize;
            if key < 128 {
                pressed[key] = true;
                colors[key] = note.color_packed;
            }
        }
    }
    ActiveKeys { pressed, colors }
}

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

/// 将颜色每个通道提亮指定值并重新打包为 `0xRRGGBBAA`。
///
/// 参考 Comet MIDITrail：激活音符在最终颜色上直接 +0.5（clamp 到 1.0）。
#[must_use]
pub fn boost_color_packed(packed: u32, amount: f32) -> u32 {
    let a = packed & 0xFF;
    let r = (((packed >> 24) & 0xFF) as f32 / 255.0 + amount).clamp(0.0, 1.0);
    let g = (((packed >> 16) & 0xFF) as f32 / 255.0 + amount).clamp(0.0, 1.0);
    let b = (((packed >> 8) & 0xFF) as f32 / 255.0 + amount).clamp(0.0, 1.0);
    (((r * 255.0) as u32) << 24) | (((g * 255.0) as u32) << 16) | (((b * 255.0) as u32) << 8) | a
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
    let scene_depth = MIDITRAIL_SCENE_DEPTH;
    let note_height = NOTE_HEIGHT;
    let note_y = NOTE_Y;
    let note_z_offset = NOTE_Z_OFFSET;
    let z_far_distance = uniform.z_far_distance.max(0.1);
    let z_far = note_z_offset - z_far_distance;

    let mut entries = Vec::with_capacity(notes.len());
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

        let visible_start = note.start_tick.max(tick);
        let visible_end = note.end_tick;
        let z_start = note_z_offset
            - ((visible_start.saturating_sub(tick)) as f32 / viewport_tick_span * scene_depth);
        let mut z_end = note_z_offset
            - ((visible_end.saturating_sub(tick)) as f32 / viewport_tick_span * scene_depth);
        z_end = z_end.max(z_far);
        if z_end >= z_start {
            continue;
        }
        let z_center = (z_start + z_end) * 0.5;
        let z_length = z_start - z_end;

        let scale = [width * 0.92, note_height, z_length];
        let translation = [left + width * 0.04, note_y, z_center - z_length * 0.5];

        let color = if note.is_active_at(tick) {
            boost_color_packed(note.color_packed, 0.5)
        } else {
            note.color_packed
        };

        entries.push((
            note.key,
            z_start,
            MiditrailInstanceGpu::new(translation, scale, color, false, 0.0, 0.0),
        ));
    }

    // 按 Comet MIDITrail 的音符绘制顺序排序：
    // 1. 白键音符先绘制，黑键音符后绘制（确保黑键音符覆盖白键音符）；
    // 2. 同颜色组内按前缘深度 far-to-near 排序，使靠近键盘的音符最后绘制。
    // 这样画家算法 + 音符不写深度，可消除重叠部分的颜色闪烁。
    entries.sort_by(|a, b| {
        let a_black = is_black_key(a.0);
        let b_black = is_black_key(b.0);
        a_black
            .cmp(&b_black)
            .then_with(|| a.1.total_cmp(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    out.extend(entries.into_iter().map(|(_, _, instance)| instance));
}

/// 构建琴键实例。
///
/// `active_keys` 由 `compute_active_keys` 预先计算，避免本函数再次扫描全部音符。
pub fn build_key_instances(
    uniform: &MiditrailUniformGpu,
    active_keys: &ActiveKeys,
    key_positions: &[f32],
    key_widths: &[f32],
    press_factors: &[f32],
    out: &mut Vec<MiditrailInstanceGpu>,
) {
    let key_count = uniform.key_count as usize;
    let key_count = key_count.min(key_positions.len());

    for i in 0..key_count {
        let left = key_positions[i];
        let width = key_widths[i];
        let is_black = is_black_key(i as u32);
        let (y, height) = if is_black {
            (BLACK_KEY_ELEVATION, BLACK_KEY_HEIGHT)
        } else {
            (0.0, KEYBOARD_HEIGHT)
        };
        let color = if active_keys.pressed[i] {
            active_keys.colors[i]
        } else if is_black {
            pack_color([0.2, 0.2, 0.2, 1.0])
        } else {
            pack_color([1.0, 1.0, 1.0, 1.0])
        };
        let depth = if is_black {
            BLACK_KEY_DEPTH
        } else {
            WHITE_KEY_DEPTH
        };
        let press_depth = if is_black {
            // 黑键按下深度最多为高出白键部分高度的 0.5，保证按下后仍可见
            (BLACK_KEY_HEIGHT - KEYBOARD_HEIGHT) * 0.5
        } else {
            KEYBOARD_HEIGHT * 0.5
        };
        let scale = [width, height, depth];
        let translation = [left, y, 0.0];
        let press = press_factors.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        out.push(MiditrailInstanceGpu::new(
            translation,
            scale,
            color,
            true,
            press,
            press_depth,
        ));
    }
}

/// 构建 Aura 实例。
///
/// 当某个键在当前 tick 有音符激活时，在对应键下方生成一个光环。
/// `active_keys` 由 `compute_active_keys` 预先计算。
pub fn build_aura_instances(
    uniform: &MiditrailUniformGpu,
    active_keys: &ActiveKeys,
    key_positions: &[f32],
    key_widths: &[f32],
    out: &mut Vec<MiditrailAuraInstanceGpu>,
) {
    let key_count = (uniform.key_count as usize)
        .min(key_positions.len())
        .min(128);

    for i in 0..key_count {
        if !active_keys.pressed[i] {
            continue;
        }
        let left = key_positions[i];
        let width = key_widths[i];
        let center = left + width * 0.5;
        // 光环半径要足够大，以环绕音符立方体（音符宽约键宽、高约 0.007）
        let size = (width * 4.0).max(0.04);
        out.push(MiditrailAuraInstanceGpu {
            size,
            pos: center,
            color_packed: active_keys.colors[i],
            _padding: 0,
        });
    }
}

#[cfg(test)]
mod tests;
