//! 标尺刻度实例生成
//!
//! 注意：背景网格线由 GPU 端 infinite_grid.wgsl 自动绘制；
//! 本节生成 CPU 侧的标尺（ruler）小节/拍线实例，支持拍号中途变化。

use crate::RulerTickInstance;

/// 判断是否为黑键
pub fn is_black_key(key_index: isize) -> bool {
    let note_in_octave = key_index.rem_euclid(12);
    matches!(note_in_octave, 1 | 3 | 6 | 8 | 10)
}

/// 拍号数据：分子、分母（人类可读）
#[derive(Debug, Clone, Copy)]
pub struct TimeSignature {
    /// 拍号分子（每小节拍数）
    pub numerator: u8,
    /// 拍号分母（以几分音符为一拍，2/4/8 等）
    pub denominator: u8,
}

impl From<(u8, u8)> for TimeSignature {
    fn from((numerator, denominator): (u8, u8)) -> Self {
        Self {
            numerator,
            denominator,
        }
    }
}

/// 返回 tick 位置所在拍号段的时间签名
fn time_signature_at(tick: f32, time_signatures: &[(u32, u8, u8)]) -> TimeSignature {
    let mut active = (4_u8, 4_u8);
    for &(ts_tick, num, den) in time_signatures {
        if tick >= ts_tick as f32 {
            active = (num, den);
        } else {
            break;
        }
    }
    active.into()
}

/// 计算给定拍号下的每拍/每小节 tick 数
fn ticks_per_beat_and_measure(ppq: u32, ts: TimeSignature) -> (u32, u32) {
    let beat_ticks = (ppq as f32 * 4.0 / ts.denominator.max(1) as f32) as u32;
    let measure_ticks = beat_ticks * ts.numerator.max(1) as u32;
    (beat_ticks, measure_ticks)
}

/// 生成标尺实例
///
/// `time_signatures` 按 tick 升序排列；空数组时回退到 4/4。
pub fn generate_ruler_instances(
    viewport_width: f32,
    keyboard_width: f32,
    ruler_height: f32,
    scroll_x: f32,
    zoom_x: f32,
    ppq: u32,
    time_signatures: &[(u32, u8, u8)],
) -> Vec<RulerTickInstance> {
    puffin::profile_function!();

    let mut instances = Vec::new();
    if time_signatures.is_empty() {
        return instances;
    }

    let visible_tick_start = scroll_x / zoom_x;
    let visible_tick_end = (scroll_x + viewport_width) / zoom_x;

    // 找到可见范围前第一个拍号变化位置，避免遗漏跨段的小节线
    let first_ts_index = time_signatures
        .iter()
        .rposition(|(tick, _, _)| *tick as f32 <= visible_tick_start)
        .unwrap_or(0);

    // 从 first_ts_index 开始向前生成，直到超出 visible_tick_end
    let mut current_tick = visible_tick_start.max(0.0) as u32;
    let mut ts_index = first_ts_index;

    while (current_tick as f32) < visible_tick_end {
        // 如果 current_tick 已越过下一段拍号边界，推进 ts_index
        while let Some((next_tick, _, _)) = time_signatures.get(ts_index + 1)
            && *next_tick <= current_tick
        {
            ts_index += 1;
        }

        let (ts_tick, _, _) = time_signatures[ts_index];
        let ts = time_signature_at(current_tick as f32, time_signatures);
        let (beat_ticks, measure_ticks) = ticks_per_beat_and_measure(ppq, ts);

        // 当前拍号段内下一个小节/拍的位置
        let next_measure_tick =
            ts_tick + (((current_tick.max(ts_tick) - ts_tick) / measure_ticks + 1) * measure_ticks);
        let next_beat_tick =
            ts_tick + (((current_tick.max(ts_tick) - ts_tick) / beat_ticks + 1) * beat_ticks);

        // 优先处理更近的事件
        let next_event_tick = next_measure_tick.min(next_beat_tick);

        // 检查是否进入下一段拍号
        let next_ts_tick = time_signatures.get(ts_index + 1).map(|(tick, _, _)| *tick);
        if let Some(next_ts) = next_ts_tick
            && next_event_tick >= next_ts
            && current_tick < next_ts
        {
            // 进入下一段前，先把当前段剩余的小节/拍线生成到 next_ts 之前
            generate_segment_lines(
                &mut instances,
                current_tick,
                next_ts,
                ts_tick,
                beat_ticks,
                measure_ticks,
                viewport_width,
                keyboard_width,
                ruler_height,
                scroll_x,
                zoom_x,
            );
            ts_index += 1;
            current_tick = next_ts;
            continue;
        }

        // 生成当前段直到 visible_tick_end 或下一段
        let segment_end = next_ts_tick.unwrap_or(visible_tick_end.ceil() as u32 + 1);
        generate_segment_lines(
            &mut instances,
            current_tick,
            segment_end,
            ts_tick,
            beat_ticks,
            measure_ticks,
            viewport_width,
            keyboard_width,
            ruler_height,
            scroll_x,
            zoom_x,
        );
        current_tick = segment_end;
    }

    instances
}

/// 生成一段拍号区间内的小节线与拍线
#[allow(clippy::too_many_arguments)]
fn generate_segment_lines(
    instances: &mut Vec<RulerTickInstance>,
    start_tick: u32,
    end_tick: u32,
    segment_ts_tick: u32,
    beat_ticks: u32,
    measure_ticks: u32,
    viewport_width: f32,
    keyboard_width: f32,
    ruler_height: f32,
    scroll_x: f32,
    zoom_x: f32,
) {
    // 小节线
    let first_measure = ((start_tick.saturating_sub(segment_ts_tick)) / measure_ticks
        + if start_tick == segment_ts_tick { 0 } else { 1 })
        * measure_ticks
        + segment_ts_tick;
    for measure_tick in (first_measure..end_tick).step_by(measure_ticks.max(1) as usize) {
        push_tick_instance(
            instances,
            measure_tick,
            viewport_width,
            keyboard_width,
            ruler_height,
            scroll_x,
            zoom_x,
            0,
            [0.3, 0.3, 0.3, 1.0],
            [2.0, ruler_height],
        );
    }

    // 拍线
    let first_beat = ((start_tick.saturating_sub(segment_ts_tick)) / beat_ticks
        + if start_tick == segment_ts_tick { 0 } else { 1 })
        * beat_ticks
        + segment_ts_tick;
    for beat_tick in (first_beat..end_tick).step_by(beat_ticks.max(1) as usize) {
        if (beat_tick - segment_ts_tick).is_multiple_of(measure_ticks) {
            continue; // 小节线已绘制
        }
        push_tick_instance(
            instances,
            beat_tick,
            viewport_width,
            keyboard_width,
            ruler_height,
            scroll_x,
            zoom_x,
            1,
            [0.5, 0.5, 0.5, 1.0],
            [1.0, ruler_height * 0.7],
        );
    }
}

/// 按拍号变化生成可见范围内的小节边界 tick（升序去重）
///
/// 走带视图网格线使用：拍号中途变化时（如 2/4 → 4/4），小节线必须
/// 跟随真实小节边界，否则网格线与音符/框选矩形错位。
/// `time_signatures` 为空时回退到固定 4/4（与旧行为一致）。
pub fn measure_line_ticks(
    visible_start: u32,
    visible_end: u32,
    ppq: u32,
    time_signatures: &[(u32, u8, u8)],
) -> Vec<u32> {
    let mut out = Vec::new();
    if time_signatures.is_empty() {
        // 回退：固定 4/4（与旧逻辑 tpb = ppq * 4 完全一致）
        let tpb = ppq.saturating_mul(4).max(1);
        let mut tick = visible_start / tpb * tpb;
        while tick <= visible_end {
            out.push(tick);
            tick = tick.saturating_add(tpb);
        }
        return out;
    }

    // 定位可见起点之前最后一个拍号段（避免遗漏跨段的小节线）
    let first_ts = time_signatures
        .iter()
        .rposition(|(tick, _, _)| *tick <= visible_start)
        .unwrap_or(0);
    let mut ts_index = first_ts;
    let mut current = visible_start;

    while current <= visible_end {
        // 推进到 current 所在拍号段
        while let Some(&(next_tick, _, _)) = time_signatures.get(ts_index + 1)
            && next_tick <= current
        {
            ts_index += 1;
        }
        let (seg_start, numerator, denominator) = time_signatures[ts_index];
        let beat = (ppq as f32 * 4.0 / denominator.max(1) as f32).max(1.0) as u32;
        let measure = beat.saturating_mul(numerator.max(1) as u32).max(1);
        let seg_end = time_signatures
            .get(ts_index + 1)
            .map(|&(next_tick, _, _)| next_tick)
            .unwrap_or(visible_end);

        // 段起点（拍号变化点）本身就是小节边界；与上一段末尾重合时去重
        if seg_start >= visible_start && seg_start <= visible_end && out.last() != Some(&seg_start)
        {
            out.push(seg_start);
        }
        // 段内后续小节边界（从段起点按 measure 对齐，排除段起点本身）
        let offset = current.max(seg_start).saturating_sub(seg_start);
        let mut boundary = seg_start + offset / measure * measure;
        if boundary <= seg_start {
            boundary = seg_start.saturating_add(measure);
        }
        while boundary <= seg_end.min(visible_end) {
            out.push(boundary);
            boundary = boundary.saturating_add(measure);
        }
        // 越过本段末尾，下一轮 while 推进 ts_index
        current = seg_end.saturating_add(1);
    }
    out
}

/// 将一条刻度线加入实例列表
#[allow(clippy::too_many_arguments)]
fn push_tick_instance(
    instances: &mut Vec<RulerTickInstance>,
    tick: u32,
    viewport_width: f32,
    keyboard_width: f32,
    _ruler_height: f32,
    scroll_x: f32,
    zoom_x: f32,
    kind: u8,
    color: [f32; 4],
    size: [f32; 2],
) {
    let screen_x = keyboard_width + tick as f32 * zoom_x - scroll_x;
    if screen_x >= keyboard_width && screen_x <= viewport_width {
        instances.push(RulerTickInstance::new(
            [screen_x, 0.0],
            size,
            color,
            kind,
            tick as f32,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ruler_instances_4_4() {
        let ppq = 480;
        let time_signatures = vec![(0, 4, 4)];
        let instances =
            generate_ruler_instances(1920.0, 60.0, 30.0, 0.0, 0.1, ppq, &time_signatures);
        // 可见 tick 范围 0..19200，4/4 下每小节 1920 tick；
        // 最右侧小节线 x=60+19200*0.1=1980 超出视口 1920，被过滤
        let measures: Vec<_> = instances.iter().filter(|i| i.tick_type == 0.0).collect();
        assert_eq!(measures.len(), 10);
    }

    #[test]
    fn test_generate_ruler_instances_3_4() {
        let ppq = 480;
        let time_signatures = vec![(0, 3, 4)];
        let instances =
            generate_ruler_instances(1920.0, 60.0, 30.0, 0.0, 0.1, ppq, &time_signatures);
        // 3/4 每小节 1440 tick
        let measures: Vec<_> = instances
            .iter()
            .filter(|i| i.tick_type == 0.0)
            .map(|i| i.tick_value)
            .collect();
        assert_eq!(measures[0], 0.0);
        assert_eq!(measures[1], 1440.0);
    }

    #[test]
    fn test_generate_ruler_instances_time_signature_change() {
        let ppq = 480;
        // 0..960 为 3/4，960 之后为 4/4
        let time_signatures = vec![(0, 3, 4), (960, 4, 4)];
        let instances =
            generate_ruler_instances(1920.0, 60.0, 30.0, 0.0, 0.1, ppq, &time_signatures);
        let measure_ticks: Vec<u32> = instances
            .iter()
            .filter(|i| i.tick_type == 0.0)
            .map(|i| i.tick_value as u32)
            .collect();
        assert!(measure_ticks.contains(&0));
        assert!(measure_ticks.contains(&960));
        assert!(measure_ticks.contains(&(960 + 1920)));
    }

    #[test]
    fn test_measure_line_ticks_fixed_4_4() {
        // 空拍号回退固定 4/4：ppq=480 → 每小节 1920
        let ticks = measure_line_ticks(0, 7680, 480, &[]);
        assert_eq!(ticks, vec![0, 1920, 3840, 5760, 7680]);
    }

    #[test]
    fn test_measure_line_ticks_short_first_measure() {
        // 第一个小节 2/4（960 ticks），后续 4/4（1920 ticks）：
        // 小节边界 0, 960, 2880, 4800 ...
        let time_signatures = vec![(0, 2, 4), (960, 4, 4)];
        let ticks = measure_line_ticks(0, 6000, 480, &time_signatures);
        assert_eq!(ticks, vec![0, 960, 2880, 4800]);
    }

    #[test]
    fn test_measure_line_ticks_mid_segment_visible() {
        // 可见范围不从 0 开始：3/4 段（0..1920 每小节 1440）→ 4/4 段
        let time_signatures = vec![(0, 3, 4), (1920, 4, 4)];
        let ticks = measure_line_ticks(1000, 6000, 480, &time_signatures);
        assert_eq!(ticks, vec![1440, 1920, 3840, 5760]);
    }
}
