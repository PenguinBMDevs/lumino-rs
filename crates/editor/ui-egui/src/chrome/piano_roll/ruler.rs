//! 标尺小节号：支持拍号中途变化（与 gfx `generate_ruler_instances` 同口径）。
//!
//! egui 标尺是**唯一**的横向标尺绘制者（gfx `RulerRenderer` 在卷帘模式被
//! `skip_roll_chrome` 跳过）。因此这里必须自己处理拍号分段，否则加载带
//! 中途变拍的 MIDI 后小节号会消失。

use super::PianoRollView;
use crate::theme;
use egui::Rect;

/// 播放指示线宽度（`PLAYBACK_INDICATOR_WIDTH`）。
const PLAYHEAD_WIDTH: f32 = 2.0;
/// 播放指示线顶部倒三角尺寸（`PLAYBACK_INDICATOR_TRIANGLE_SIZE`）。
const PLAYHEAD_TRIANGLE: f32 = 8.0;

/// 可见小节号：`(相对标尺左缘的 x, 小节号)`。
///
/// 拍号列表按 tick 升序；同 tick 重复折叠为最后一条（与 gfx 标尺的
/// `ts_index` 推进 / `time_signature_at` 的「后者覆盖」一致）。空列表或
/// 首段不在 tick 0 时补默认 4/4，保证 tick 0 起始终有小节号。
pub(super) fn measure_labels(view: &PianoRollView, width: f32) -> Vec<(f32, u64)> {
    let zoom_x = view.zoom_x.max(f32::EPSILON);
    let start_tick = (view.scroll_x / zoom_x).max(0.0);
    let end_tick = ((view.scroll_x + width) / zoom_x).max(start_tick);

    // 同 tick 后者覆盖前者；空列表 = 4/4。
    let mut segments: Vec<(u32, u8, u8)> = Vec::new();
    for &(tick, numerator, denominator) in &view.time_signatures {
        match segments.last_mut() {
            Some(last) if last.0 == tick => {
                last.1 = numerator;
                last.2 = denominator;
            }
            _ => segments.push((tick, numerator, denominator)),
        }
    }
    if segments.first().is_none_or(|&(tick, _, _)| tick != 0) {
        segments.insert(0, (0, 4, 4));
    }

    let mut labels = Vec::new();
    // 当前段之前累计的小节数（全局小节号 = measures_before + 段内索引 + 1）。
    let mut measures_before: u64 = 0;
    for (index, &(seg_tick, numerator, denominator)) in segments.iter().enumerate() {
        let next_tick = segments.get(index + 1).map(|&(tick, _, _)| tick);
        let ticks_per_measure =
            f32::from(view.ppq) * 4.0 * f32::from(numerator) / f32::from(denominator.max(1));
        if ticks_per_measure <= 0.0 {
            continue;
        }
        let segment_start = seg_tick as f32;
        let segment_end = next_tick.map_or(f32::INFINITY, |tick| tick as f32);
        // 本段完整包含的小节数（供下一段累计全局小节号）。
        let segment_measures = if segment_end.is_finite() {
            ((segment_end - segment_start) / ticks_per_measure)
                .ceil()
                .max(0.0) as u64
        } else {
            0
        };
        // 整段都在视口左侧：只累计小节号，不生成标签。
        if segment_end <= start_tick {
            measures_before += segment_measures;
            continue;
        }

        // 从视口左侧最近的小节开始（可能落在段首之前，但 k 从 0 起）。
        let first_k = ((start_tick - segment_start) / ticks_per_measure)
            .floor()
            .max(0.0) as u64;
        let mut k = first_k;
        loop {
            let measure_tick = segment_start + k as f32 * ticks_per_measure;
            // 多留一个小节，避免视口左边界刚好切在小节线时漏标签。
            if measure_tick > end_tick + ticks_per_measure || measure_tick >= segment_end {
                break;
            }
            let x = measure_tick * zoom_x - view.scroll_x;
            if x >= 0.0 && x <= width {
                labels.push((x, measures_before + k + 1));
            }
            k += 1;
            if k - first_k > 10_000 {
                break;
            }
        }
        measures_before += segment_measures;
    }
    labels
}

/// 播放指示线（标尺行内的线 + 顶部倒三角）。
pub(super) fn playhead(ui: &egui::Ui, rect: Rect, view: &PianoRollView) {
    if view.playhead_offset < 0.0 {
        return;
    }
    let center = rect.left() + view.playhead_offset;
    if center > rect.right() {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(
        Rect::from_min_size(
            egui::pos2(center - PLAYHEAD_WIDTH / 2.0, rect.top()),
            egui::Vec2::new(PLAYHEAD_WIDTH, rect.height()),
        ),
        0,
        theme::playhead(),
    );
    let half = PLAYHEAD_TRIANGLE / 2.0;
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(center - half, rect.top()),
            egui::pos2(center + half, rect.top()),
            egui::pos2(center, rect.top() + PLAYHEAD_TRIANGLE),
        ],
        theme::playhead(),
        egui::Stroke::NONE,
    ));
}

#[cfg(test)]
mod tests {
    use super::measure_labels;
    use crate::chrome::piano_roll::PianoRollView;

    fn view(signatures: Vec<(u32, u8, u8)>) -> PianoRollView {
        PianoRollView {
            ppq: 1920,
            zoom_x: 0.1,
            time_signatures: signatures,
            ..Default::default()
        }
    }

    #[test]
    fn single_meter_labels_every_measure() {
        let labels = measure_labels(&view(vec![(0, 4, 4)]), 2000.0);
        assert_eq!(labels, vec![(0.0, 1), (768.0, 2), (1536.0, 3)]);
    }

    #[test]
    fn duplicate_time_signatures_collapse_to_last() {
        // 多轨 MIDI 常见：同 tick 重复 4/4；同 tick 不同值时后者覆盖。
        let labels = measure_labels(&view(vec![(0, 4, 4), (0, 4, 4)]), 2000.0);
        assert_eq!(labels, vec![(0.0, 1), (768.0, 2), (1536.0, 3)]);

        // 3/4：每小节 5760 tick → 576px。
        let labels = measure_labels(&view(vec![(0, 4, 4), (0, 3, 4)]), 2000.0);
        assert_eq!(labels, vec![(0.0, 1), (576.0, 2), (1152.0, 3), (1728.0, 4)]);
    }

    #[test]
    fn mid_project_change_continues_measure_numbers() {
        // 4/4 一小节 @7680，然后 3/4：小节号必须连续，不能回到 1。
        let labels = measure_labels(&view(vec![(0, 4, 4), (7680, 3, 4)]), 2000.0);
        assert_eq!(
            labels,
            vec![
                (0.0, 1),
                (768.0, 2),
                (1344.0, 3), // 7680 + 5760 = 13440 tick → 1344px
                (1920.0, 4),
            ]
        );
    }

    #[test]
    fn empty_signatures_fall_back_to_common_time() {
        let labels = measure_labels(&view(Vec::new()), 2000.0);
        assert_eq!(labels, vec![(0.0, 1), (768.0, 2), (1536.0, 3)]);
    }
}
