//! 自动化贝塞尔曲线：控制柄重算、插值求值、弯音密集采样
//!
//! 与卷帘曲线工具（ui-editor `interaction/line_tool/geom.rs`）的三次贝塞尔
//! 语义一致：段由前事件出向柄 + 后事件入向柄控制；自动柄（1/3 段长）时
//! 退化为精确直线。此处为纯数据层算法，供渲染（gfx）与弯音应用（播放
//! 采样、MIDI 导出）复用。

use super::{AutomationEvent, AutomationLane, SegmentShape};

/// 采样上限保护：单次采样的事件数上限，超出时均匀降采样（防御极端工程
/// 数据——如单段跨越数十万 tick）。正常工程远低于此值。
pub const MAX_BEND_SAMPLE_EVENTS: usize = 2_000_000;

/// 三次贝塞尔求值（多项式形式），返回 `t ∈ [0,1]` 处的值坐标。
///
/// cp1 = 前事件出向柄绝对位置，cp2 = 后事件入向柄绝对位置。
/// 两端柄为自动（1/3 段方向）时退化为线性插值（精确直线）。
#[inline]
pub fn bezier_value(a: &AutomationEvent, b: &AutomationEvent, t: f32) -> f32 {
    let y0 = a.value as f32;
    let y1 = b.value as f32;
    let cp1 = a.out_handle_abs().1;
    let cp2 = b.in_handle_abs().1;
    let u = 1.0 - t;
    u * u * u * y0 + 3.0 * u * u * t * cp1 + 3.0 * u * t * t * cp2 + t * t * t * y1
}

impl AutomationLane {
    /// 重算全部自动控制柄：相邻事件间的柄取段方向 1/3 长度
    /// （三次贝塞尔直线条件，保证未弯曲段外观为直线）。
    ///
    /// 仅重算 `handles_auto`（未被用户自定义）的柄；
    /// 用户拖动过的柄保持原值不被覆盖。编辑（增删移事件）后调用。
    pub fn recompute_auto_handles(&mut self) {
        for i in 0..self.events.len().saturating_sub(1) {
            let a = self.events[i];
            let b = self.events[i + 1];
            if a.handles_auto {
                self.events[i].out_handle = (
                    (b.tick as f32 - a.tick as f32) / 3.0,
                    (b.value as f32 - a.value as f32) / 3.0,
                );
            }
            if b.handles_auto {
                self.events[i + 1].in_handle = (
                    (a.tick as f32 - b.tick as f32) / 3.0,
                    (a.value as f32 - b.value as f32) / 3.0,
                );
            }
        }
    }

    /// 将 lane 密集采样为 `(tick, value)` 事件序列（弯音应用）。
    ///
    /// 采样规则：
    /// - `Step` 段：值保持到下一事件（不产生中间事件，合成器保持语义）；
    /// - `Curve` 段：**按 tick 逐点采样**三次贝塞尔曲线（自动柄 = 直线，
    ///   自定义柄 = 实际弯曲），每个 tick 一个事件；
    /// - 相邻同值合并（值未变化不重复发送事件）；
    /// - 末尾事件必定包含；
    /// - 超过 `max_events` 时均匀降采样（保留首尾），防御极端数据。
    pub fn sample_curve(&self, max_events: usize) -> Vec<(u32, u16)> {
        let events = &self.events;
        if events.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<(u32, u16)> = Vec::new();
        out.push((events[0].tick, events[0].value));
        for pair in events.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            match a.shape {
                SegmentShape::Step => {
                    // 终点为新值生效点：输出 b（同值合并由下方统一去重处理）
                    out.push((b.tick, b.value));
                }
                SegmentShape::Curve { .. } => {
                    let span = b.tick.saturating_sub(a.tick);
                    if span > 0 {
                        for dt in 1..=span {
                            let t = dt as f32 / span as f32;
                            let y = bezier_value(a, b, t).round().clamp(0.0, 16383.0) as u16;
                            out.push((a.tick + dt, y));
                        }
                    }
                }
            }
        }
        // 相邻同值合并（值未变化不重复发送事件）
        let mut deduped: Vec<(u32, u16)> = Vec::with_capacity(out.len());
        for &(tick, value) in &out {
            if deduped.last().is_some_and(|&(_, v)| v == value) {
                continue;
            }
            deduped.push((tick, value));
        }
        // 上限保护：均匀抽样（严格不超过 max_events，且必含首尾）
        let n = deduped.len();
        let max = max_events.max(2);
        if n > max {
            let mut sampled = Vec::with_capacity(max);
            for k in 0..max {
                let idx = k * (n - 1) / (max - 1);
                sampled.push(deduped[idx]);
            }
            deduped = sampled;
        }
        deduped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{AutomationTarget, SegmentShape};

    fn event(tick: u32, value: u16) -> AutomationEvent {
        AutomationEvent::new(tick, value, SegmentShape::Curve { tension: 0 })
    }

    fn lane(events: Vec<AutomationEvent>) -> AutomationLane {
        let mut lane = AutomationLane {
            target: AutomationTarget::PitchBend,
            track: 0,
            channel: 0,
            events,
        };
        lane.recompute_auto_handles();
        lane
    }

    #[test]
    fn test_recompute_auto_handles_line() {
        // 自动柄 = 1/3 段长 → 直线
        let mut l = lane(vec![event(0, 0), event(960, 960)]);
        assert_eq!(l.events[0].out_handle, (320.0, 320.0));
        assert_eq!(l.events[1].in_handle, (-320.0, -320.0));
        // 用户自定义柄不被覆盖
        l.events[1].set_in_handle((-100.0, -200.0));
        l.recompute_auto_handles();
        assert_eq!(l.events[1].in_handle, (-100.0, -200.0));
        assert_eq!(l.events[0].out_handle, (320.0, 320.0));
    }

    #[test]
    fn test_bezier_value_line_degenerates_to_linear() {
        // 自动柄 → 直线：中点值 = 两端均值
        let l = lane(vec![event(0, 0), event(960, 960)]);
        let (a, b) = (&l.events[0], &l.events[1]);
        assert!((bezier_value(a, b, 0.0) - 0.0).abs() < 1e-3);
        assert!((bezier_value(a, b, 0.5) - 480.0).abs() < 1e-3);
        assert!((bezier_value(a, b, 1.0) - 960.0).abs() < 1e-3);
    }

    #[test]
    fn test_bezier_value_bent_handle() {
        // 出向柄拉高 → 曲线中点高于直线中点
        let mut a = event(0, 0);
        a.set_out_handle((320.0, 600.0));
        let mut b = event(960, 0);
        b.set_in_handle((-320.0, 600.0));
        let mid = bezier_value(&a, &b, 0.5);
        assert!(mid > 400.0, "弯曲曲线中点应显著高于直线中点 0，实际 {mid}");
        assert!(mid < 960.0);
    }

    #[test]
    fn test_sample_curve_step_keeps_value() {
        // Step 段不产生中间事件；末尾事件保留
        let mut l = lane(vec![
            AutomationEvent::new(0, 50, SegmentShape::Step),
            AutomationEvent::new(960, 100, SegmentShape::Step),
            AutomationEvent::new(1920, 0, SegmentShape::Step),
        ]);
        l.recompute_auto_handles();
        let samples = l.sample_curve(10_000);
        assert_eq!(samples, vec![(0, 50), (960, 100), (1920, 0)]);
    }

    #[test]
    fn test_sample_curve_line_every_tick_dedup() {
        // 直线段 0→960：中间值每 tick 变化（相邻去重后保留全部变化点）
        let mut l = lane(vec![event(0, 0), event(10, 100)]);
        l.recompute_auto_handles();
        let samples = l.sample_curve(10_000);
        assert_eq!(samples.len(), 11, "0..=10 每个 tick 一个变化点");
        assert_eq!(samples[0], (0, 0));
        assert_eq!(samples[10], (10, 100));
        assert_eq!(samples[5], (5, 50), "直线中点 = 两端均值");
        // tick 单调递增
        assert!(samples.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn test_sample_curve_flat_middle_dedup() {
        // 值变化只发生在部分 tick：同值合并
        let mut a = event(0, 0);
        a.set_out_handle((3.33, 50.0)); // 快速冲到 ~90
        let mut b = event(10, 100);
        b.set_in_handle((-3.33, 50.0));
        let mut l = lane(vec![a, b]);
        l.recompute_auto_handles();
        let samples = l.sample_curve(10_000);
        // 两端柄自动被重算（上边 set 已标记自定义，不会被覆盖）
        assert!(!l.events[0].handles_auto);
        assert!(!l.events[1].handles_auto);
        // 曲线值区间 [0, 100]，相邻 tick 值可能相同 → 去重后事件数 <= 11
        assert!(samples.len() <= 11);
        assert_eq!(samples.first(), Some(&(0, 0)));
        assert_eq!(samples.last(), Some(&(10, 100)));
    }

    #[test]
    fn test_sample_curve_max_events_downsample() {
        // 全值域大跨度直线（值档 16384 > 上限 100）→ 均匀抽样且保留首尾值
        let mut l = lane(vec![event(0, 0), event(100_000, 16_383)]);
        l.recompute_auto_handles();
        let samples = l.sample_curve(100);
        assert!(samples.len() <= 100);
        assert_eq!(samples.first(), Some(&(0, 0)));
        // 末尾事件值必须为终点值（同值合并可能让生效 tick 略早于终点）
        assert_eq!(samples.last().map(|&(_, v)| v), Some(16_383));
        assert!(samples.last().is_some_and(|&(t, _)| t <= 100_000));
    }

    #[test]
    fn test_sample_curve_empty() {
        let l = lane(Vec::new());
        assert!(l.sample_curve(100).is_empty());
    }

    #[test]
    fn test_sample_curve_single_event() {
        let l = lane(vec![event(0, 8192)]);
        assert_eq!(l.sample_curve(100), vec![(0, 8192)]);
    }

    #[test]
    fn test_default_event_is_auto_handle() {
        let e = AutomationEvent::default();
        assert!(e.handles_auto);
    }

    #[test]
    fn test_sample_curve_loopback_no_duplicate_tick() {
        // 防御：旧数据可能绕过 setter 直接构造回环柄（出向柄越过锚点）。
        // 采样层必须保证 tick 严格单调——单个 tick 绝不包含多个弯音事件。
        let mut a = event(0, 8192);
        a.out_handle = (-500.0, 8000.0); // 直接字段赋值模拟历史坏数据
        a.handles_auto = false;
        let mut b = event(960, 8192);
        b.in_handle = (500.0, 8000.0);
        b.handles_auto = false;
        let mut l = lane(vec![a, b]);
        l.recompute_auto_handles(); // 自定义柄不被覆盖（handles_auto=false）
        let samples = l.sample_curve(10_000);
        assert!(samples.len() > 1);
        // tick 严格单调递增（去重后仍保持）
        assert!(
            samples.windows(2).all(|w| w[0].0 < w[1].0),
            "回环曲线采样不得产生重复 tick 事件: {samples:?}"
        );
        // 值全部钳制在 14-bit 范围内
        assert!(samples.iter().all(|&(_, v)| v <= 16383));
    }

    #[test]
    fn test_set_handle_prevents_loopback_in_sampling() {
        // 新数据经 setter 钳制后：出向柄 tick 偏移 >= 0 → 曲线无回环
        let mut a = event(0, 8192);
        a.set_out_handle((-500.0, 8000.0)); // 越界输入被钳制
        let mut b = event(960, 8192);
        b.set_in_handle((500.0, 8000.0));
        assert_eq!(a.out_handle.0, 0.0);
        assert_eq!(b.in_handle.0, 0.0);
        let mut l = lane(vec![a, b]);
        l.recompute_auto_handles();
        let samples = l.sample_curve(10_000);
        // 钳制后出向柄在锚点垂直切线上：t=0 附近曲线可能仍非单调（大 value 偏移），
        // 但 tick 必须严格单调
        assert!(samples.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
