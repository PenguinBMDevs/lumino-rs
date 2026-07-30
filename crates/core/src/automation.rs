//! 自动化/控制器数据模型
//!
//! 从 yinhe 项目移植的 AutomationLane 数据模型，统一描述 CC、PitchBend、RPN、NRPN
//! 等可自动化参数的时序事件，并支持 Step / Curve（三次贝塞尔）两种插值形状。

use crate::midi_types::PITCH_BEND_CENTER;
use serde::{Deserialize, Serialize};

/// 段插值形状：描述从一个事件到下一个事件的过渡方式。
///
/// 存储在每个事件的 `shape` 字段上，描述“从本事件开始”的线段的插值方式。
/// 最后一个事件的 shape 无实际作用（后面没有线段）。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum SegmentShape {
    /// 离散：保持当前值直到下个事件才瞬间跳变。与 MIDI CC 原生语义一致。
    Step,
    /// 三次贝塞尔曲线（CSS handle 风格，偏移量参数化）。
    ///
    /// 归一化空间：起点 P0=(0,0) 对应本事件，终点 P3=(1,1) 对应下一事件。
    /// 存储值为控制点相对各自锚点的归一化偏移量，内部 `*4` 放大得到实际贝塞尔参数：
    ///
    /// - `(x1, y1)`：P1 相对 P0 的偏移，实际位置 P1 = P0 + (P3-P0)·(x1·4, y1·4)
    /// - `(x2, y2)`：P2 相对 P3 的偏移，实际位置 P2 = P3 + (P3-P0)·(x2·4, y2·4)
    ///
    /// 每个分量 `∈ [-0.5, 0.5]`，内部 `*4` 后实际参数范围 `[-2, 2]`。
    /// 直线（退化）：`(0, 0, 0, 0)` - 0 为中性，偏离 0 即弯曲。
    Curve { x1: f32, y1: f32, x2: f32, y2: f32 },
}

impl Default for SegmentShape {
    /// MIDI 导入与未指定时的默认值。Step 与 MIDI CC 原生语义一致。
    fn default() -> Self {
        SegmentShape::Step
    }
}

impl SegmentShape {
    /// 偏移量参数化的放大系数：存储值 `[-0.5, 0.5]` × 4 = 实际参数 `[-2, 2]`。
    pub const SCALE: f32 = 4.0;

    /// 直线 Curve 的默认偏移量：全部为 0（中性）。
    pub const LINEAR_X1: f32 = 0.0;
    pub const LINEAR_Y1: f32 = 0.0;
    pub const LINEAR_X2: f32 = 0.0;
    pub const LINEAR_Y2: f32 = 0.0;

    /// 直线 Curve 的快捷构造。
    pub const fn linear_curve() -> Self {
        SegmentShape::Curve {
            x1: Self::LINEAR_X1,
            y1: Self::LINEAR_Y1,
            x2: Self::LINEAR_X2,
            y2: Self::LINEAR_Y2,
        }
    }

    /// 在归一化进度 `t ∈ [0, 1]` 上计算插值因子 `f ∈ [0, 1]`。
    /// `value_at = v1 + (v2 - v1) * f`。
    ///
    /// 对于 Curve，t 是 tick 进度。三次贝塞尔的参数 u 不等于 t，
    /// 需要从 x(u)=t 反解 u（数值法），再代入 y(u)。
    #[inline]
    pub fn interpolate(self, t: f32) -> f32 {
        debug_assert!((0.0..=1.0).contains(&t), "interpolate t out of range: {t}");
        let t = t.clamp(0.0, 1.0);
        match self {
            SegmentShape::Step => 0.0, // Step: hold v1 until next event
            SegmentShape::Curve { x1, y1, x2, y2 } => {
                if Self::is_linear_impl(x1, y1, x2, y2) {
                    return t;
                }
                // 实际控制点（归一化空间，P0=(0,0), P3=(1,1)）：
                // P1 = (x1*4, y1*4), P2 = (1+x2*4, 1+y2*4)
                let u = solve_cubic_bezier_u_for_x(t, x1, x2);
                let u1 = 1.0 - u;
                let p1y = y1 * Self::SCALE;
                let p2y = 1.0 + y2 * Self::SCALE;
                3.0 * u1 * u1 * u * p1y + 3.0 * u1 * u * u * p2y + u * u * u
            }
        }
    }

    /// 是否为直线（Curve 且偏移量全部 ≈ 0）。
    #[inline]
    pub fn is_linear(self) -> bool {
        matches!(self, SegmentShape::Curve { x1, y1, x2, y2 }
            if Self::is_linear_impl(x1, y1, x2, y2))
    }

    #[inline]
    fn is_linear_impl(x1: f32, y1: f32, x2: f32, y2: f32) -> bool {
        x1.abs() < 1e-4 && y1.abs() < 1e-4 && x2.abs() < 1e-4 && y2.abs() < 1e-4
    }
}

/// 解三次贝塞尔方程 B_x(u) = t 求 u（Newton 迭代）。
///
/// 偏移量参数化：P1.x = x1·4，P2.x = 1 + x2·4。
/// 初值用 u=t（直线时精确）。6 次迭代对 [0,1] 范围足够收敛。
#[inline]
fn solve_cubic_bezier_u_for_x(t: f32, x1: f32, x2: f32) -> f32 {
    let p1x = x1 * SegmentShape::SCALE;
    let p2x = 1.0 + x2 * SegmentShape::SCALE;
    let mut u = t.clamp(0.0, 1.0);
    for _ in 0..6 {
        let u1 = 1.0 - u;
        let f = 3.0 * u1 * u1 * u * p1x + 3.0 * u1 * u * u * p2x + u * u * u - t;
        let df = 3.0 * u1 * u1 * p1x + 6.0 * u1 * u * (p2x - p1x) + 3.0 * u * u * (1.0 - p2x);
        if df.abs() < 1e-6 {
            break;
        }
        u -= f / df;
        u = u.clamp(0.0, 1.0);
    }
    u
}

/// 可自动化参数的标识。
///
/// 这是所有自动化数据的统一键 -- CC、PitchBend、RPN、NRPN 等。
/// 每个 variant 映射到一条按 tick 排序的 `(tick, value)` 事件 lane。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AutomationTarget {
    /// MIDI CC 0–127。
    CC { controller: u8 },
    /// MIDI Pitch Bend (0–16383, 中心 8192)。
    PitchBend,
    /// RPN (Registered Parameter Number)，14-bit 参数地址 0–16383。
    Rpn { parameter: u16 },
    /// NRPN (Non-Registered Parameter Number)，14-bit 参数地址 0–16383。
    Nrpn { parameter: u16 },
}

impl AutomationTarget {
    /// 该目标是否使用完整的 14-bit 范围 (0–16383)。
    ///
    /// RPN 0 (Pitch Bend Sensitivity) 与 RPN 2 (Coarse Tune) 为 7-bit 值 (0–127)。
    /// 只有 RPN 1 (Fine Tune) 是 14-bit。
    pub fn is_14bit(&self) -> bool {
        matches!(
            self,
            AutomationTarget::PitchBend
                | AutomationTarget::Rpn { parameter: 1 }
                | AutomationTarget::Nrpn { .. }
        )
    }

    /// 该目标的原始最大值（用于将数值归一化为面板高度）。
    pub fn max_value(&self) -> u16 {
        match self {
            AutomationTarget::CC { .. } => 127,
            AutomationTarget::PitchBend => 16383,
            AutomationTarget::Rpn { parameter } => match parameter {
                0 => 127,   // Pitch Bend Sensitivity (semitones)
                2 => 127,   // Coarse Tune (semitones, -64..+63 stored as 0..127)
                _ => 16383, // Fine Tune (14-bit)
            },
            AutomationTarget::Nrpn { .. } => 16383,
        }
    }

    /// 默认值 / 中心值（用于绘制参考线）。
    pub fn default_value(&self) -> u16 {
        match self {
            AutomationTarget::CC { controller } => match controller {
                10 | 71 | 72 | 73 | 74 => 64,
                _ => 0,
            },
            AutomationTarget::PitchBend => PITCH_BEND_CENTER as u16,
            AutomationTarget::Rpn { parameter } => match parameter {
                0 => 2,                        // Pitch Bend Sensitivity (2 semitones)
                1 => PITCH_BEND_CENTER as u16, // Fine Tune (center of 14-bit range)
                _ => 0,
            },
            AutomationTarget::Nrpn { .. } => 0,
        }
    }

    /// 该目标是否有非零中心线（PitchBend、Fine Tune 等）。
    pub fn has_center_line(&self) -> bool {
        matches!(
            self,
            AutomationTarget::PitchBend
                | AutomationTarget::Rpn { parameter: 1 }
                | AutomationTarget::CC { controller: 10 }
                | AutomationTarget::CC { controller: 71 }
                | AutomationTarget::CC { controller: 72 }
                | AutomationTarget::CC { controller: 73 }
                | AutomationTarget::CC { controller: 74 }
        )
    }

    /// 编辑器新建事件时本目标默认采用的插值形状。
    ///
    /// - 开关类 CC（Sustain/Sostenuto/Soft/Legato/Portamento）默认 `Step`
    /// - 其他连续量（Volume/Pan/PB/FineTune/...）默认 `Curve` 直线（偏移量 0,0,0,0）
    /// - MIDI 导入时一律使用 `Step`（保留 MIDI 原生语义）
    pub fn default_shape(&self) -> SegmentShape {
        match self {
            AutomationTarget::CC { controller } => match controller {
                64..=68 => SegmentShape::Step,
                _ => SegmentShape::linear_curve(),
            },
            AutomationTarget::PitchBend => SegmentShape::linear_curve(),
            AutomationTarget::Rpn { parameter: _ } => SegmentShape::linear_curve(),
            AutomationTarget::Nrpn { parameter: _ } => SegmentShape::linear_curve(),
        }
    }

    /// 人类可读显示名称（用于下拉框）。
    pub fn display_name(&self) -> String {
        match self {
            AutomationTarget::CC { controller } => {
                let name = cc_name(*controller);
                if name.is_empty() {
                    format!("CC {}", controller)
                } else {
                    format!("CC {} ({})", controller, name)
                }
            }
            AutomationTarget::PitchBend => "Pitch Bend".into(),
            AutomationTarget::Rpn { parameter } => match parameter {
                0 => "PB Sensitivity (RPN 0)".into(),
                1 => "Fine Tune (RPN 1)".into(),
                2 => "Coarse Tune (RPN 2)".into(),
                _ => format!("RPN {}", parameter),
            },
            AutomationTarget::Nrpn { parameter } => {
                format!("NRPN {}", parameter)
            }
        }
    }
}

/// 通用 MIDI CC 名称（标准 GM/GS 分配）。
fn cc_name(cc: u8) -> &'static str {
    match cc {
        0 => "Bank Select MSB",
        1 => "Mod Wheel",
        2 => "Breath",
        4 => "Foot",
        5 => "Portamento Time",
        6 => "Data Entry MSB",
        7 => "Volume",
        8 => "Balance",
        10 => "Pan",
        11 => "Expression",
        32 => "Bank Select LSB",
        38 => "Data Entry LSB",
        64 => "Sustain",
        65 => "Portamento",
        66 => "Sostenuto",
        67 => "Soft Pedal",
        68 => "Legato",
        71 => "Resonance",
        72 => "Release",
        73 => "Attack",
        74 => "Cutoff",
        84 => "Portamento Control",
        91 => "Reverb",
        92 => "Tremolo",
        93 => "Chorus",
        94 => "Detune",
        95 => "Phaser",
        100 => "RPN LSB",
        101 => "RPN MSB",
        _ => "",
    }
}

/// 单个自动化事件：某个时间点的值。
///
/// channel 与 track 不存储在此，由所属的 `AutomationLane` 隐含。
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct AutomationEvent {
    /// tick 位置（MIDI 脉冲）。
    pub tick: u32,
    /// 原始值。范围取决于目标（0–127 for CC, 0–16383 for PB 等）。
    pub value: u16,
    /// 描述“从本事件到下一事件”的插值形状。
    #[serde(default)]
    pub shape: SegmentShape,
}

impl AutomationEvent {
    /// 构造一个使用目标默认 shape 的事件。
    pub fn with_default_shape(tick: u32, value: u16, target: &AutomationTarget) -> Self {
        Self {
            tick,
            value,
            shape: target.default_shape(),
        }
    }
}

/// 某个 track 上某个参数的一条有序自动化事件 lane。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationLane {
    pub target: AutomationTarget,
    /// 音轨索引（与 `EditorData.current_track` 等对应）。
    pub track: u16,
    /// MIDI 通道号（0-15）。从 MIDI 文件导入时保留原始通道，
    /// 用户新建事件时默认为 0。
    pub channel: u8,
    /// 按 tick 排序的事件列表。
    pub events: Vec<AutomationEvent>,
}

impl AutomationLane {
    /// 返回 tick 落在 `[start_tick, end_tick)` 范围内的事件切片。
    pub fn events_in_range(&self, start_tick: u32, end_tick: u32) -> &[AutomationEvent] {
        let lo = self.events.partition_point(|e| e.tick < start_tick);
        let hi = self.events.partition_point(|e| e.tick < end_tick);
        &self.events[lo..hi]
    }

    /// Chase：找到 `target_tick` 之前的最后一个事件值。
    ///
    /// 若之前无事件则返回 `None`。
    pub fn chase_value(&self, target_tick: u32) -> Option<u16> {
        let idx = self.events.partition_point(|e| e.tick < target_tick);
        if idx > 0 {
            Some(self.events[idx - 1].value)
        } else {
            None
        }
    }

    /// 用事件 tick/value/shape 计算一个稳定的哈希值，用于渲染缓存键。
    pub fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.track.hash(&mut hasher);
        self.channel.hash(&mut hasher);
        self.target.hash(&mut hasher);
        self.events.len().hash(&mut hasher);
        for e in &self.events {
            e.tick.hash(&mut hasher);
            e.value.hash(&mut hasher);
            match e.shape {
                SegmentShape::Step => 0u8.hash(&mut hasher),
                SegmentShape::Curve { x1, y1, x2, y2 } => {
                    1u8.hash(&mut hasher);
                    x1.to_bits().hash(&mut hasher);
                    y1.to_bits().hash(&mut hasher);
                    x2.to_bits().hash(&mut hasher);
                    y2.to_bits().hash(&mut hasher);
                }
            }
        }
        hasher.finish()
    }
}

/// 自动化编辑操作。
///
/// 由 UI 交互层产生，应用到 `EditorData` 的自动化数据。
#[derive(Clone, Debug)]
pub enum AutomationEdit {
    /// 添加新事件。若 lane 不存在则自动创建。
    Add {
        track_idx: u16,
        target: AutomationTarget,
        /// MIDI 通道号（0-15）。
        channel: u8,
        tick: u32,
        value: u16,
        shape: SegmentShape,
    },
    /// 移动已有事件。
    Move {
        track_idx: u16,
        lane_idx: usize,
        old_tick: u32,
        new_tick: u32,
        new_value: u16,
    },
    /// 切换已有事件的 shape（双击）。
    CycleShape {
        track_idx: u16,
        lane_idx: usize,
        tick: u32,
    },
    /// 直接设置已有事件的 shape（用于贝塞尔控制点拖拽）。
    SetShape {
        track_idx: u16,
        lane_idx: usize,
        tick: u32,
        shape: SegmentShape,
    },
    /// 删除指定事件。
    Delete {
        track_idx: u16,
        lane_idx: usize,
        tick: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_lane(target: AutomationTarget, ticks: &[u32]) -> AutomationLane {
        AutomationLane {
            target,
            track: 0,
            channel: 0,
            events: ticks
                .iter()
                .map(|&t| AutomationEvent {
                    tick: t,
                    value: 64,
                    shape: SegmentShape::Step,
                })
                .collect(),
        }
    }

    #[test]
    fn test_events_in_range() {
        let lane = make_lane(
            AutomationTarget::CC { controller: 7 },
            &[100, 200, 300, 400, 500],
        );
        let slice = lane.events_in_range(150, 450);
        assert_eq!(slice.len(), 3);
        assert_eq!(slice[0].tick, 200);
        assert_eq!(slice[2].tick, 400);
    }

    #[test]
    fn test_events_in_range_empty() {
        let lane = make_lane(AutomationTarget::CC { controller: 7 }, &[100, 200]);
        assert!(lane.events_in_range(300, 400).is_empty());
    }

    #[test]
    fn test_chase_value_found() {
        let lane = AutomationLane {
            target: AutomationTarget::CC { controller: 7 },
            track: 0,
            channel: 0,
            events: vec![
                AutomationEvent {
                    tick: 100,
                    value: 80,
                    shape: SegmentShape::Step,
                },
                AutomationEvent {
                    tick: 200,
                    value: 100,
                    shape: SegmentShape::Step,
                },
                AutomationEvent {
                    tick: 300,
                    value: 60,
                    shape: SegmentShape::Step,
                },
            ],
        };
        assert_eq!(lane.chase_value(250), Some(100));
        assert_eq!(lane.chase_value(300), Some(100));
    }

    #[test]
    fn test_chase_value_none() {
        let lane = make_lane(AutomationTarget::CC { controller: 7 }, &[200, 300]);
        assert_eq!(lane.chase_value(100), None);
    }

    #[test]
    fn test_segment_shape_interpolate_endpoints() {
        // Step 在区间内始终返回 0（值仍为 v1，由调用方处理）
        assert_eq!(SegmentShape::Step.interpolate(0.0), 0.0);
        assert_eq!(SegmentShape::Step.interpolate(0.5), 0.0);
        assert_eq!(SegmentShape::Step.interpolate(1.0), 0.0);

        // 直线 Curve（偏移量全 0）端点和中点
        let lin = SegmentShape::linear_curve();
        assert_eq!(lin.interpolate(0.0), 0.0);
        assert_eq!(lin.interpolate(1.0), 1.0);
        assert!((lin.interpolate(0.5) - 0.5).abs() < 1e-6);

        // 贝塞尔端点：无论控制点位置，端点始终为 0 和 1
        assert_eq!(
            SegmentShape::Curve {
                x1: 0.1,
                y1: 0.2,
                x2: -0.1,
                y2: -0.2
            }
            .interpolate(0.0),
            0.0
        );
        assert_eq!(
            SegmentShape::Curve {
                x1: 0.1,
                y1: 0.2,
                x2: -0.1,
                y2: -0.2
            }
            .interpolate(1.0),
            1.0
        );
    }

    #[test]
    fn test_segment_shape_bezier_midpoint() {
        // 直线（偏移量全 0）：B_y(0.5) = 0.5
        assert!((SegmentShape::linear_curve().interpolate(0.5) - 0.5).abs() < 1e-6);

        // ease-in-out 近似：偏移量 (0.105, 0, -0.105, 0)，B_y(0.5) 接近 0.5
        let ease_io = SegmentShape::Curve {
            x1: 0.105,
            y1: 0.0,
            x2: -0.105,
            y2: 0.0,
        };
        let v = ease_io.interpolate(0.5);
        assert!(
            (v - 0.5).abs() < 0.02,
            "ease-in-out mid expected ~0.5, got {v}"
        );

        // 控制点全部偏到 v_end：B_y(0.5) = 0.875
        let v_end = SegmentShape::Curve {
            x1: 0.075,
            y1: 0.25,
            x2: -0.075,
            y2: 0.0,
        }
        .interpolate(0.5);
        assert!((v_end - 0.875).abs() < 1e-6, "expected 0.875, got {v_end}");

        // 控制点全部偏到 v_start：B_y(0.5) = 0.125
        let v_start = SegmentShape::Curve {
            x1: 0.075,
            y1: 0.0,
            x2: -0.075,
            y2: -0.25,
        }
        .interpolate(0.5);
        assert!(
            (v_start - 0.125).abs() < 1e-6,
            "expected 0.125, got {v_start}"
        );
    }

    #[test]
    fn test_segment_shape_is_linear() {
        assert!(SegmentShape::linear_curve().is_linear());
        assert!(
            !SegmentShape::Curve {
                x1: 0.0,
                y1: 0.1,
                x2: 0.0,
                y2: 0.0
            }
            .is_linear()
        );
        assert!(!SegmentShape::Step.is_linear());
    }

    #[test]
    fn test_target_max_and_default_values() {
        assert_eq!(AutomationTarget::CC { controller: 0 }.max_value(), 127);
        assert_eq!(AutomationTarget::CC { controller: 0 }.default_value(), 0);
        assert_eq!(AutomationTarget::CC { controller: 10 }.default_value(), 64);
        assert_eq!(AutomationTarget::PitchBend.max_value(), 16383);
        assert_eq!(
            AutomationTarget::PitchBend.default_value(),
            PITCH_BEND_CENTER as u16
        );
        assert!(AutomationTarget::PitchBend.has_center_line());
        assert!(!AutomationTarget::CC { controller: 7 }.has_center_line());
    }

    #[test]
    fn test_default_shape_per_target() {
        for cc in [64u8, 65, 66, 67, 68] {
            assert_eq!(
                AutomationTarget::CC { controller: cc }.default_shape(),
                SegmentShape::Step,
                "CC {cc} should default to Step"
            );
        }
        let linear = SegmentShape::linear_curve();
        for cc in [0u8, 1, 7, 10, 11, 71, 74] {
            assert_eq!(
                AutomationTarget::CC { controller: cc }.default_shape(),
                linear,
                "CC {cc} should default to linear Curve"
            );
        }
        assert_eq!(AutomationTarget::PitchBend.default_shape(), linear);
    }

    #[test]
    fn test_automation_event_with_default_shape() {
        let evt =
            AutomationEvent::with_default_shape(100, 64, &AutomationTarget::CC { controller: 7 });
        assert_eq!(evt.shape, SegmentShape::linear_curve());

        let evt2 =
            AutomationEvent::with_default_shape(100, 0, &AutomationTarget::CC { controller: 64 });
        assert_eq!(evt2.shape, SegmentShape::Step);
    }
}
