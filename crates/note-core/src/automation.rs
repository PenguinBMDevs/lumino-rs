//! 自动化/控制器数据模型
//!
//! 从 yinhe 项目移植的 AutomationLane 数据模型，统一描述 CC、PitchBend、RPN、NRPN
//! 等可自动化参数的时序事件，并支持 Step / Curve 两种插值形状。
//!
//! 贝塞尔曲线几何（控制柄重算、插值求值、密集采样）见 [`curve`] 子模块。

pub mod curve;

use crate::midi_types::PITCH_BEND_CENTER;
use serde::{Deserialize, Serialize};

/// 段插值形状：描述从一个事件到下一个事件的过渡方式。
///
/// 存储在每个事件的 `shape` 字段上，描述"从本事件开始"的线段的插值方式。
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
    pub fn interpolate(self, normalized_t: f32) -> f32 {
        debug_assert!(
            (0.0..=1.0).contains(&normalized_t),
            "interpolate t out of range: {normalized_t}"
        );
        let t = normalized_t.clamp(0.0, 1.0);
        match self {
            SegmentShape::Step => 0.0,
            SegmentShape::Curve { tension } => {
                let tension_norm = (tension as f32) / 127.0; // [-1, 1]
                if tension_norm >= 0.0 {
                    // 慢起快落: 线性 → x²
                    (1.0 - tension_norm) * t + tension_norm * t * t
                } else {
                    // 快起慢落: 线性 → 1 - (1-x)²
                    let tension_norm = -tension_norm;
                    (1.0 - tension_norm) * t + tension_norm * (1.0 - (1.0 - t).powi(2))
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
///
/// 事件带贝塞尔控制柄（与卷帘曲线工具的 `BezierAnchor` 同构）：
/// - `handles_auto = true`（默认）：柄由 `AutomationLane::recompute_auto_handles`
///   自动维护（取相邻段方向 1/3 = 三次贝塞尔精确直线），段外观为直线；
/// - 用户拖动控制柄后标记自定义（`handles_auto = false`），段按实际柄弯曲。
///
/// 旧数据（无柄字段）反序列化时自动回退为自动柄，语义不变。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AutomationEvent {
    /// tick 位置（MIDI 脉冲）。
    pub tick: u32,
    /// 原始值。范围取决于目标（0–127 for CC, 0–16383 for PB 等）。
    pub value: u16,
    /// 描述"从本事件到下一事件"的插值形状。
    #[serde(default)]
    pub shape: SegmentShape,
    /// 出向控制柄偏移（相对 tick/value，控制"到下一事件"的贝塞尔段）。
    #[serde(default)]
    pub out_handle: (f32, f32),
    /// 入向控制柄偏移（相对 tick/value，控制"来自上一事件"的贝塞尔段）。
    #[serde(default)]
    pub in_handle: (f32, f32),
    /// 控制柄是否自动维护（未被用户自定义）：`true` 时重算覆盖柄为直线。
    #[serde(default = "default_true")]
    pub handles_auto: bool,
}

impl Default for AutomationEvent {
    /// 默认事件为自动柄（直线段语义），与 `AutomationEvent::new` 一致。
    fn default() -> Self {
        Self::new(0, 0, SegmentShape::default())
    }
}

fn default_true() -> bool {
    true
}

impl AutomationEvent {
    /// 构造事件（控制柄自动维护，偏移为 0——由 lane 重算填充）。
    pub fn new(tick: u32, value: u16, shape: SegmentShape) -> Self {
        Self {
            tick,
            value,
            shape,
            out_handle: (0.0, 0.0),
            in_handle: (0.0, 0.0),
            handles_auto: true,
        }
    }

    /// 构造一个使用目标默认 shape 的事件。
    pub fn with_default_shape(tick: u32, value: u16, target: &AutomationTarget) -> Self {
        Self::new(tick, value, target.default_shape())
    }

    /// 出向控制柄绝对坐标（逻辑坐标：tick, value）。
    pub fn out_handle_abs(&self) -> (f32, f32) {
        (
            self.tick as f32 + self.out_handle.0,
            self.value as f32 + self.out_handle.1,
        )
    }

    /// 入向控制柄绝对坐标（逻辑坐标：tick, value）。
    pub fn in_handle_abs(&self) -> (f32, f32) {
        (
            self.tick as f32 + self.in_handle.0,
            self.value as f32 + self.in_handle.1,
        )
    }

    /// 设置出向控制柄（标记为自定义，不再自动维护）。
    ///
    /// **钳制规则**：出向柄的 tick 偏移不允许 < 0（不能越过锚点垂直切线），
    /// 防止贝塞尔曲线回环——回环会导致同一 tick 区间内曲线上下往返
    /// （视觉多条弯音曲线、播放弯音错乱）。
    pub fn set_out_handle(&mut self, offset: (f32, f32)) {
        self.out_handle = (offset.0.max(0.0), offset.1);
        self.handles_auto = false;
    }

    /// 设置入向控制柄（标记为自定义，不再自动维护）。
    ///
    /// **钳制规则**：入向柄的 tick 偏移不允许 > 0（不能越过锚点垂直切线），
    /// 防止贝塞尔曲线回环（同 [`AutomationEvent::set_out_handle`]）。
    pub fn set_in_handle(&mut self, offset: (f32, f32)) {
        self.in_handle = (offset.0.min(0.0), offset.1);
        self.handles_auto = false;
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
        let start_idx = self.events.partition_point(|e| e.tick < start_tick);
        let end_idx = self.events.partition_point(|e| e.tick < end_tick);
        &self.events[start_idx..end_idx]
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
        for event in &self.events {
            event.tick.hash(&mut hasher);
            event.value.hash(&mut hasher);
            match event.shape {
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
        /// 旧值（精确匹配用）：弯音跳变对（同 tick 两事件）场景传
        /// `Some(原值)` 按 tick+value 定位目标；其他场景传 `None`
        /// 仅按 tick 匹配（同 tick 唯一）。
        old_value: Option<u16>,
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
    /// 更新已有事件的贝塞尔控制柄（实时拖柄用）。
    ///
    /// `handles_auto` 传入 `false`（拖柄 = 自定义柄）；如需恢复自动柄
    /// 由调用方用 `SetHandlesAuto`（当前未暴露，拖柄即标记自定义）。
    UpdateHandles {
        track_idx: u16,
        lane_idx: usize,
        tick: u32,
        out_handle: (f32, f32),
        in_handle: (f32, f32),
    },
    /// 清空指定 lane 的全部事件（√× 确认模式全量重建用）。
    Clear { track_idx: u16, lane_idx: usize },
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
                .map(|&t| AutomationEvent::new(t, 64, SegmentShape::Step))
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
                AutomationEvent::new(100, 80, SegmentShape::Step),
                AutomationEvent::new(200, 100, SegmentShape::Step),
                AutomationEvent::new(300, 60, SegmentShape::Step),
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

    #[test]
    fn test_set_handle_clamps_tick_offset() {
        // 出向柄：tick 偏移不允许 < 0（越过锚点垂直切线 = 曲线回环）
        let mut a = AutomationEvent::new(0, 8192, SegmentShape::Curve { tension: 0 });
        a.set_out_handle((-500.0, 3000.0));
        assert_eq!(a.out_handle.0, 0.0, "出向柄 tick 偏移被钳制为 0");
        assert_eq!(a.out_handle.1, 3000.0, "value 偏移不受限");

        // 入向柄：tick 偏移不允许 > 0
        let mut b = AutomationEvent::new(960, 8192, SegmentShape::Curve { tension: 0 });
        b.set_in_handle((500.0, -3000.0));
        assert_eq!(b.in_handle.0, 0.0, "入向柄 tick 偏移被钳制为 0");
        assert_eq!(b.in_handle.1, -3000.0);

        // 合法偏移不受影响
        let mut c = AutomationEvent::new(0, 8192, SegmentShape::Curve { tension: 0 });
        c.set_out_handle((320.0, 3000.0));
        assert_eq!(c.out_handle.0, 320.0);
        let mut d = AutomationEvent::new(960, 8192, SegmentShape::Curve { tension: 0 });
        d.set_in_handle((-320.0, -3000.0));
        assert_eq!(d.in_handle.0, -320.0);
    }
}
