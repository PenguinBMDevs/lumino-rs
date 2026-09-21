use super::*;

/// 构建 GPU-Driven 管线参数（`MiditrailDrivenParamsGpu`）。
///
/// 视口/深度公式与 `build_note_instances` 头部逐 op 一致（ppq/speed 钳位、
/// `visible_measure_count` 取整、span 下限 1），键位表直拷调用方缓存。
/// Top 视图不走此路径（沿用 CPU `quantize_notes_for_top`＋legacy 构建）。
pub fn build_driven_params(
    uniform: &MiditrailUniformGpu,
    key_positions: &[f32],
    key_widths: &[f32],
) -> MiditrailDrivenParamsGpu {
    let ppq = uniform.ppq.max(1);
    let speed = uniform.speed.max(0.1);
    let ticks_per_measure = ppq * 4;
    let visible_measure_count = ((4.0 / speed).round()).max(1.0) as u32;
    let viewport_tick_span = (ticks_per_measure * visible_measure_count).max(1) as f32;
    let z_far = NOTE_Z_OFFSET - uniform.z_far_distance.max(0.1);
    let mut key_table = [[0.0f32; 4]; 128];
    let n = key_positions.len().min(key_widths.len()).min(128);
    for (i, slot) in key_table.iter_mut().enumerate().take(n) {
        *slot = [key_positions[i], key_widths[i], 0.0, 0.0];
    }
    MiditrailDrivenParamsGpu {
        tick: uniform.tick,
        viewport_tick_span,
        scene_depth: MIDITRAIL_SCENE_DEPTH,
        note_z_offset: NOTE_Z_OFFSET,
        z_far,
        note_height: NOTE_HEIGHT,
        note_y: NOTE_Y,
        key_count: uniform.key_count,
        key_table,
    }
}
