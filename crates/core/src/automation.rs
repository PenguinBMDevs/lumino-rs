//! 自动化/控制器数据模型
//!
//! 从 yinhe 项目移植的 AutomationLane 数据模型，统一描述 CC、PitchBend、RPN、NRPN
//! 等可自动化参数的时序事件，并支持 Step / Curve 两种插值形状。

use crate::midi_types::PITCH_BEND_CENTER;
use serde::{Deserialize, Serialize};

/// 段插值形状：描述从一个事件到下一个事件的过渡方式。
///
/// 存储在每个事件的 `shape` 字段上，描述“从本事件开始”的线段的插值方式。
/// 最后一个事件的 shape 无实际作用（后面没有线段）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SegmentShape {
    /// 离散：保持当前值直到下个事件才瞬间跳变。与 MIDI CC 原生语义一致。
    Step,
    /// 曲线：tension 控制曲线弯曲方向与程度。
    /// - `0` 等价于直线
    /// - `> 0` 慢起快落（ease-in）
    /// - `< 0` 快起慢落（ease-out）
    ///   范围为 -127..=127。
    Curve { tension: i8 },
}

impl Default for SegmentShape {
    /// MIDI 导入与未指定时的默认值。Step 与 MIDI CC 原生语义一致。
    fn default() -> Self {
        SegmentShape::Step
    }
}

impl SegmentShape {
    /// 在归一化进度 `t ∈ [0, 1]` 上计算插值因子 `f ∈ [0, 1]`。
    /// 实际值 = v1 + (v2 - v1) * f。
    #[inline]
    pub fn interpolate(self, t: f32) -> f32 {
        debug_assert!((0.0..=1.0).contains(&t), "interpolate t out of range: {t}");
        let t = t.clamp(0.0, 1.0);
        match self {
            SegmentShape::Step => 0.0,
            SegmentShape::Curve { tension } => {
                let k = (tension as f32) / 127.0; // [-1, 1]
                if k >= 0.0 {
                    // 慢起快落: 线性 → x²
                    (1.0 - k) * t + k * t * t
                } else {
                    // 快起慢落: 线性 → 1 - (1-x)²
                    let k = -k;
                    (1.0 - k) * t + k * (1.0 - (1.0 - t).powi(2))
                }
            }
        }
    }
}

/// 可自动化参数的标识。
///
/// 这是所有自动化数据的统一键 —— CC、PitchBend、RPN、NRPN 等。
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
    /// - 其他连续量（Volume/Pan/PB/FineTune/...）默认 `Curve { tension: 0 }`（=直线）
    /// - MIDI 导入时一律使用 `Step`（保留 MIDI 原生语义）
    pub fn default_shape(&self) -> SegmentShape {
        match self {
            AutomationTarget::CC { controller } => match controller {
                64..=68 => SegmentShape::Step,
                _ => SegmentShape::Curve { tension: 0 },
            },
            AutomationTarget::PitchBend => SegmentShape::Curve { tension: 0 },
            AutomationTarget::Rpn { parameter: _ } => SegmentShape::Curve { tension: 0 },
            AutomationTarget::Nrpn { parameter: _ } => SegmentShape::Curve { tension: 0 },
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
                SegmentShape::Curve { tension } => {
                    1u8.hash(&mut hasher);
                    tension.hash(&mut hasher);
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
        let lin0 = SegmentShape::Curve { tension: 0 };
        assert_eq!(lin0.interpolate(0.0), 0.0);
        assert_eq!(lin0.interpolate(1.0), 1.0);
        assert!((lin0.interpolate(0.5) - 0.5).abs() < 1e-6);

        assert_eq!(SegmentShape::Step.interpolate(0.5), 0.0);
    }

    #[test]
    fn test_curve_direction() {
        let ease_in = SegmentShape::Curve { tension: 127 }.interpolate(0.5);
        assert!(ease_in < 0.5);

        let ease_out = SegmentShape::Curve { tension: -127 }.interpolate(0.5);
        assert!(ease_out > 0.5);
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
        for cc in [0u8, 1, 7, 10, 11, 71, 74] {
            assert_eq!(
                AutomationTarget::CC { controller: cc }.default_shape(),
                SegmentShape::Curve { tension: 0 },
                "CC {cc} should default to Curve{{tension:0}}"
            );
        }
    }

    #[test]
    fn test_automation_event_with_default_shape() {
        let evt =
            AutomationEvent::with_default_shape(100, 64, &AutomationTarget::CC { controller: 7 });
        assert_eq!(evt.shape, SegmentShape::Curve { tension: 0 });

        let evt2 =
            AutomationEvent::with_default_shape(100, 0, &AutomationTarget::CC { controller: 64 });
        assert_eq!(evt2.shape, SegmentShape::Step);
    }
}
