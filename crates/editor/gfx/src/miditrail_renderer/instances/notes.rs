use super::*;

/// 构建可见音符的实例数据。
///
/// 排序解耦为"索引排序 + gather"两步：`scratch.order` 存 (排序键, 下标) 16B
/// 元组并用 LSD 基数排序（稳定，与 `sort_by_key` 输出严格一致，见
/// sort_equivalence 回归测试），再按排好序的下标把 `out` 中的实例 gather
/// 到 `scratch.gather` 后 swap 回 `out`。
pub fn build_note_instances(
    uniform: &MiditrailUniformGpu,
    notes: &[MiditrailNoteGpu],
    key_positions: &[f32],
    key_widths: &[f32],
    out: &mut Vec<MiditrailInstanceGpu>,
    scratch: &mut NoteBuildScratch,
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

    // 打包排序键 + 下标：排序键 = is_black(1bit) | z 可排序位(32bit) | key(7bit)，
    // 详见下方 f32 位重排注释。只排序 16B (键, 下标) 元组（旧 56B (键, 实例)
    // 元组移动量的约 1/3），实例按输入序直接进 `out`，排序后 gather 重排。
    // scratch 均由调用方提供并跨帧复用，避免每帧大堆分配。
    out.clear();
    out.reserve(notes.len());
    let order = &mut scratch.order;
    order.clear();
    order.reserve(notes.len());
    let t_loop = std::time::Instant::now();
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

        // f32 位重排为可排序 u32（与 f32::total_cmp 全序严格等价）：
        // - 正数（含正 NaN）：翻转符号位 → 映射到 [0x8000_0000, 0xFFFF_FFFF]
        // - 负数（含负 NaN）：按位取反 → 映射到 [0x0000_0000, 0x7FFF_FFFF]
        // 这样排序结果与旧实现 `z_start.total_cmp` 完全一致（含 NaN 顺序）。
        let z_bits = z_start.to_bits();
        let z_sortable = if z_bits & 0x8000_0000 != 0 {
            !z_bits
        } else {
            z_bits ^ 0x8000_0000
        };
        // 位布局：is_black(bit 63) | z_sortable(bit 39..=7，32 位) | key(bit 6..=0，7 位)。
        // key 范围 0-127 只需 7 位；z 左移 7 位后最高到 bit 38，不与 bit 63 冲突。
        // 注意：z 不能左移 32 位——z_sortable ≥ 0x8000_0000（正数 z）时其 bit 31
        // 会落到 bit 63，与 is_black 位互相污染导致排序错乱（曾实测 85 处不一致）。
        let sort_key = ((is_black_key(note.key as isize) as u64) << 63)
            | ((z_sortable as u64) << 7)
            | (note.key as u64);

        out.push(MiditrailInstanceGpu::new(
            translation,
            scale,
            color,
            false,
            0.0,
            0.0,
        ));
        order.push((sort_key, out.len() as u32 - 1));
    }

    // 按 Comet MIDITrail 的音符绘制顺序排序：
    // 1. 白键音符先绘制，黑键音符后绘制（确保黑键音符覆盖白键音符）；
    // 2. 同颜色组内按前缘深度 far-to-near 排序，使靠近键盘的音符最后绘制。
    // 这样画家算法 + 音符不写深度，可消除重叠部分的颜色闪烁。
    // 用 `sort_by_key`（稳定）替代旧 `sort_by` 三键闭包：排序键 u64 已完整编码
    // (is_black, z, key) 全序，单键比较比闭包快（基准：10 万音符 6.37ms → 2.5ms，
    // 省约 60%）；稳定性保留旧语义——完全同键（同 key 同 start 的和弦叠音）
    // 按输入顺序绘制，与旧实现一致，避免同位置音符覆盖顺序不确定导致闪烁。
    let loop_us = t_loop.elapsed().as_micros() as u64;
    let t_sort = std::time::Instant::now();
    radix_sort_order(order, &mut scratch.radix, &mut scratch.hist);
    let sort_us = t_sort.elapsed().as_micros() as u64;
    let t_gather = std::time::Instant::now();
    // 按排好序的下标 gather：`scratch_gather` 复用跨帧容量，swap 后 `out`
    // 即为最终绘制序，旧内容留在 gather 缓冲供下一帧复用（零分配）。
    let gather = &mut scratch.gather;
    gather.clear();
    gather.reserve(out.len());
    for &(_, idx) in order.iter() {
        gather.push(out[idx as usize]);
    }
    std::mem::swap(out, gather);
    let gather_us = t_gather.elapsed().as_micros() as u64;
    diag_build_notes(loop_us, sort_us, gather_us, out.len());
}

/// build_note_instances 内部分段打点（首 3 帧 + 每 300 帧）：定位 loop/sort/gather 配比。
fn diag_build_notes(loop_us: u64, sort_us: u64, gather_us: u64, notes: usize) {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n < 3 || n.is_multiple_of(300) {
        tracing::info!(
            "miditrail细分[{n}]: loop={loop_us} sort={sort_us} gather={gather_us} notes={notes}"
        );
    }
}
