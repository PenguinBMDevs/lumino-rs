use super::*;

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
        .filter(|key_idx| !is_black_key(*key_idx as isize))
        .count()
        .max(1);
    let white_width = 1.0 / num_white as f32;
    let black_width = white_width * BLACK_KEY_WIDTH_RATIO;

    let mut pos = 0.0f32;
    for key_idx in 0..count {
        if is_black_key(key_idx as isize) {
            key_positions[key_idx] = pos - black_width * 0.5;
            key_widths[key_idx] = black_width;
        } else {
            key_positions[key_idx] = pos;
            key_widths[key_idx] = white_width;
            pos += white_width;
        }
    }
}

/// 构建琴键实例。
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

    for key_idx in 0..key_count {
        let left = key_positions[key_idx];
        let width = key_widths[key_idx];
        let is_black = is_black_key(key_idx as isize);
        let (y, height) = if is_black {
            (BLACK_KEY_ELEVATION, BLACK_KEY_HEIGHT)
        } else {
            (0.0, KEYBOARD_HEIGHT)
        };
        let color = if active_keys.pressed[key_idx] {
            active_keys.colors[key_idx]
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
        let press = press_factors
            .get(key_idx)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
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
