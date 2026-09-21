use super::*;

/// 构建 Aura 实例（音符光晕环放大动画）。
///
/// 参考 Zenith-MIDI `MidiTrailRender/Render.cs` 的 `auraSize` 累加逻辑，
/// 每个键的光晕尺寸由该键上**正在发声**的音符实时驱动：
/// - **按下闪光**：音符起始后 `AURA_FLASH_FRAMES` 帧内二次衰减的冲击分量
///   `(max(10 - 起始帧数, 0))² / 600`，按下瞬间光环放大到最大再回落到常态；
/// - **常态/收缩**：`(min(剩余时长, 1s) / min(音符时长, 1s))^0.3 / 2`，
///   长音符保持 0.5，临近结束时（最后 1 秒）光环收缩到 0，平滑消失；
/// - 光环半径 = 键宽 × `AURA_RING_SCALE` × 光晕系数（Zenith 的
///   `circleRadius * 12 * auraSize` 按视觉反馈缩到 2/3）。
///
/// 同键多个音符取光晕系数最大值；环颜色沿用 `active_keys` 的按下键颜色。
/// 每帧完全由 (tick, notes) 重算，不依赖跨帧状态，seek/变速均自洽。
pub fn build_aura_instances(
    uniform: &MiditrailUniformGpu,
    notes: &[MiditrailNoteGpu],
    active_keys: &ActiveKeys,
    key_positions: &[f32],
    key_widths: &[f32],
    out: &mut Vec<MiditrailAuraInstanceGpu>,
) {
    let key_count = (uniform.key_count as usize)
        .min(key_positions.len())
        .min(128);

    // 每键光晕系数：该键正在发声的音符贡献的最大值（Zenith `auraSize[k]`）
    let mut aura_sizes = [0.0f32; 128];
    for note in notes {
        let key = note.key as usize;
        if key >= key_count {
            continue;
        }
        let factor = aura_factor_for_note(uniform, note);
        if factor > aura_sizes[key] {
            aura_sizes[key] = factor;
        }
    }

    for key_idx in 0..key_count {
        // 颜色仅来自当前按下的键（与 compute_active_keys 一致）；
        // aura_sizes[key] > 0 等价于该键有正在发声的音符。
        if !active_keys.pressed[key_idx] {
            continue;
        }
        let aura = aura_sizes[key_idx];
        if aura <= 0.0 {
            continue;
        }
        let width = key_widths[key_idx];
        let center = key_positions[key_idx] + width * 0.5;
        let size = (width * AURA_RING_SCALE * aura).max(0.001);
        out.push(MiditrailAuraInstanceGpu {
            size,
            pos: center,
            color_packed: active_keys.colors[key_idx],
            _padding: 0,
        });
    }
}

/// 单个音符对所在键光晕系数的贡献（Zenith `factor + factor2`）。
///
/// 仅当音符当前正在发声（`start_tick <= tick < end_tick`）且已开始时有贡献；
/// 未开始的音符（Zenith `n.start < midiTime` 才累加）与已结束的音符直接返回 0。
fn aura_factor_for_note(uniform: &MiditrailUniformGpu, note: &MiditrailNoteGpu) -> f32 {
    aura_factor_raw(
        uniform.tick,
        uniform.ticks_per_second,
        uniform.fps,
        note.start_tick,
        note.end_tick,
    )
}

/// 光晕系数核心数学（`tick/tps/fps/start/end` 五元决定，与载体无关）。
///
/// Legacy `MiditrailNoteGpu` 路径与 GPU-Driven `NoteInstance` 路径共用，
/// 保证两条路径的光晕动画逐位一致。
fn aura_factor_raw(
    tick: u32,
    ticks_per_second: f32,
    fps: f32,
    start_tick: u32,
    end_tick: u32,
) -> f32 {
    if start_tick > tick || end_tick <= tick {
        return 0.0;
    }
    let tps = ticks_per_second.max(0.1);
    // Zenith `tempoFrameStep`：每帧 tick 数 = 每秒 tick 数 / fps
    let frame_ticks = (tps / fps.max(1.0)).max(0.001);

    // 按下闪光：起始后 AURA_FLASH_FRAMES 帧内二次衰减到 0
    let frames_since_start = (tick - start_tick) as f32 / frame_ticks;
    let flash = (AURA_FLASH_FRAMES - frames_since_start).max(0.0).powi(2) / AURA_FLASH_DIVISOR;

    // 常态/收缩：长音符保持 AURA_HELD_FACTOR，最后 AURA_TAIL_SECONDS 内收缩到 0。
    // Zenith `maxAuraLen = tempoFrameStep * fps` 即每秒 tick 数，作为收缩窗口。
    let aura_len = tps * AURA_TAIL_SECONDS;
    let length = (end_tick - start_tick).max(1) as f32;
    let remaining = (end_tick - tick) as f32;
    let offset = remaining.min(aura_len);
    let len = length.min(aura_len);
    let tail = if len > 0.0 {
        (offset / len).powf(AURA_TAIL_POWER) * AURA_HELD_FACTOR
    } else {
        0.0
    };

    tail + flash
}

/// GPU-Driven 路径：单次遍历 `NoteInstance` 同时产出按键状态与每键光晕系数。
///
/// 与 legacy 两次全量扫描（`compute_active_keys`＋`build_aura_instances`内循环）
/// 数学等价：`start/end` 解码与 `render_from_instances` 换算逐 op 一致
/// （`max(0)`钳位＋`max(1)`长度），同键多音符取最后颜色、光晕取最大。
/// 返回 `(ActiveKeys, aura_sizes[128])`，调用方用 `emit_aura_instances` 落盘。
pub fn compute_active_and_aura_for_compact(
    tick: u32,
    ticks_per_second: f32,
    fps: f32,
    notes: &[NoteInstance],
) -> (ActiveKeys, [f32; 128]) {
    let mut pressed = [false; 128];
    let mut colors = [0u32; 128];
    let mut aura_sizes = [0.0f32; 128];
    for n in notes {
        let key = (n.key_color & 0xFF) as usize;
        if key >= 128 {
            continue;
        }
        // 与 `render_from_instances` 换算一致：start 钳零，长度至少 1 tick。
        let start = n.start_length[0].max(0.0) as u32;
        let end = start.saturating_add(n.start_length[1].max(1.0) as u32);
        if start <= tick && tick < end {
            pressed[key] = true;
            // `key_color` 高 24 位即 RGB，低 8 key 字节清零后补 alpha=0xFF，
            // 与 legacy `unpack→pack_color` 逐字节一致。
            colors[key] = (n.key_color & 0xFFFF_FF00) | 0xFF;
            let factor = aura_factor_raw(tick, ticks_per_second, fps, start, end);
            if factor > aura_sizes[key] {
                aura_sizes[key] = factor;
            }
        }
    }
    (ActiveKeys { pressed, colors }, aura_sizes)
}

/// 由 `(ActiveKeys, aura_sizes)` 落盘 Aura 实例（GPU-Driven 路径）。
///
/// 与 `build_aura_instances` 后半段（`pressed` 门控＋环几何）逐 op 一致，
/// 只是输入已由 `compute_active_and_aura_for_compact` 预聚合，省一次全量扫描。
pub fn emit_aura_instances(
    active_keys: &ActiveKeys,
    aura_sizes: &[f32; 128],
    key_count: usize,
    key_positions: &[f32],
    key_widths: &[f32],
    out: &mut Vec<MiditrailAuraInstanceGpu>,
) {
    let key_count = key_count
        .min(key_positions.len())
        .min(key_widths.len())
        .min(128);
    for key_idx in 0..key_count {
        if !active_keys.pressed[key_idx] {
            continue;
        }
        let aura = aura_sizes[key_idx];
        if aura <= 0.0 {
            continue;
        }
        let width = key_widths[key_idx];
        let center = key_positions[key_idx] + width * 0.5;
        let size = (width * AURA_RING_SCALE * aura).max(0.001);
        out.push(MiditrailAuraInstanceGpu {
            size,
            pos: center,
            color_packed: active_keys.colors[key_idx],
            _padding: 0,
        });
    }
}
