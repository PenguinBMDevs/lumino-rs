//! MIDI 编辑领域模型
//!
//! 力度/Tempo/CC 编辑面板使用的数据类型。

/// 编辑模式
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// 力度编辑
    #[default]
    Velocity,
    /// 速度编辑（Conductor 音轨专用）
    Tempo,
    /// 弯音编辑（-8192 到 +8191）
    Bend,
    /// CC 控制器编辑
    Cc(u8),
}

impl EditMode {
    /// 所有可用的 EditMode 变体（用于切换）
    pub fn all_modes() -> Vec<Self> {
        vec![Self::Velocity]
    }

    /// 获取显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            EditMode::Velocity => "力度",
            EditMode::Tempo => "速度",
            EditMode::Bend => "Bend",
            EditMode::Cc(_) => "CC",
        }
    }

    /// 是否处于 CC 模式（包括 Bend）
    pub fn is_cc(&self) -> bool {
        matches!(self, EditMode::Cc(_) | EditMode::Bend)
    }

    /// 是否处于 Tempo 模式
    pub fn is_tempo(&self) -> bool {
        matches!(self, EditMode::Tempo)
    }
}

/// CC 控制点
#[derive(Debug, Clone, Copy)]
pub struct CcPoint {
    /// tick 位置
    pub tick: f32,
    /// 控制器值 (0-127)
    pub value: u8,
}

/// 弯音 14-bit 范围中心值
///
/// MIDI Pitch Bend 值范围为 0–16383，中心（无弯音）为 8192。
/// 对应键盘编辑器中的有符号范围 -8192..+8191。
pub const PITCH_BEND_CENTER: i16 = 8192;

/// 弯音控制点
#[derive(Debug, Clone, Copy)]
pub struct BendPoint {
    /// tick 位置
    pub tick: f32,
    /// 弯音值 (-8192 到 +8191)
    pub value: i16,
}

/// 音轨 CC 数据
#[derive(Debug, Clone, Default)]
pub struct CcData {
    /// 控制器编号 → 控制点列表
    pub controllers: std::collections::HashMap<u8, Vec<CcPoint>>,
    /// 弯音点列表
    pub bend_points: Vec<BendPoint>,
}

/// 力度点数据
#[derive(Debug, Clone, Copy)]
pub struct VelocityPoint {
    /// 在 notes 向量中的索引
    pub note_index: usize,
    /// 音符的起始 tick（用于排序）
    pub tick: f32,
    /// 力度值 0-127
    pub velocity: u8,
    /// 音符长度（tick），用于柱状条宽度计算
    pub length: f32,
}

/// 已知 CC 控制器名称（GM/GS/XG 标准）
pub const CC_CONTROLLER_NAMES: &[(u8, &str)] = &[
    (0, "Bank Select MSB"),
    (1, "Modulation Wheel"),
    (2, "Breath Controller"),
    (4, "Foot Controller"),
    (5, "Portamento Time"),
    (6, "Data Entry MSB"),
    (7, "Channel Volume"),
    (8, "Balance"),
    (10, "Pan"),
    (11, "Expression"),
    (12, "Effect Control 1"),
    (13, "Effect Control 2"),
    (16, "General Purpose 1"),
    (17, "General Purpose 2"),
    (18, "General Purpose 3"),
    (19, "General Purpose 4"),
    (32, "Bank Select LSB"),
    (33, "Modulation Wheel LSB"),
    (34, "Breath Controller LSB"),
    (36, "Foot Controller LSB"),
    (37, "Portamento Time LSB"),
    (38, "Data Entry LSB"),
    (39, "Channel Volume LSB"),
    (40, "Balance LSB"),
    (42, "Pan LSB"),
    (43, "Expression LSB"),
    (64, "Sustain Pedal"),
    (65, "Portamento On/Off"),
    (66, "Sostenuto Pedal"),
    (67, "Soft Pedal"),
    (68, "Legato Footswitch"),
    (69, "Hold 2"),
    (70, "Sound Variation"),
    (71, "Resonance"),
    (72, "Release Time"),
    (73, "Attack Time"),
    (74, "Brightness / Cutoff"),
    (75, "Sound Controller 6"),
    (76, "Sound Controller 7"),
    (77, "Sound Controller 8"),
    (78, "Sound Controller 9"),
    (79, "Sound Controller 10"),
    (80, "General Purpose 5"),
    (81, "General Purpose 6"),
    (82, "General Purpose 7"),
    (83, "General Purpose 8"),
    (84, "Portamento Control"),
    (91, "Reverb Depth"),
    (92, "Tremolo Depth"),
    (93, "Chorus Depth"),
    (94, "Celeste Depth"),
    (95, "Phaser Depth"),
    (96, "Data Increment"),
    (97, "Data Decrement"),
    (98, "NRPN LSB"),
    (99, "NRPN MSB"),
    (100, "RPN LSB"),
    (101, "RPN MSB"),
    (120, "All Sound Off"),
    (121, "Reset All Controllers"),
    (122, "Local Control On/Off"),
    (123, "All Notes Off"),
    (124, "Omni Off"),
    (125, "Omni On"),
    (126, "Mono On"),
    (127, "Poly On"),
];

/// CC 编号显示包装（下拉框显示 "编号: 名称"）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CcDisplay(pub u8);

/// 弯音显示包装（下拉框显示 "Bend: Pitch Bend (-8192..8191)"）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BendDisplay;

impl std::fmt::Display for BendDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bend: Pitch Bend (-8192..8191)")
    }
}

impl std::fmt::Display for CcDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match CC_CONTROLLER_NAMES.iter().find(|(n, _)| *n == self.0) {
            Some((_, name)) => write!(f, "{}: {}", self.0, name),
            None => write!(f, "{}", self.0),
        }
    }
}

/// 速度控制点
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoPoint {
    /// 速度点所在位置的 tick。
    pub tick: f32,
    /// 该点的速度值（BPM）。
    pub bpm: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_mode_default_is_velocity() {
        assert_eq!(EditMode::default(), EditMode::Velocity);
    }

    #[test]
    fn test_edit_mode_display_name() {
        assert_eq!(EditMode::Velocity.display_name(), "力度");
        assert_eq!(EditMode::Tempo.display_name(), "速度");
        assert_eq!(EditMode::Bend.display_name(), "Bend");
        assert_eq!(EditMode::Cc(7).display_name(), "CC");
    }

    #[test]
    fn test_edit_mode_is_cc() {
        assert!(!EditMode::Velocity.is_cc());
        assert!(!EditMode::Tempo.is_cc());
        assert!(EditMode::Bend.is_cc());
        assert!(EditMode::Cc(1).is_cc());
    }

    #[test]
    fn test_edit_mode_is_tempo() {
        assert!(!EditMode::Velocity.is_tempo());
        assert!(EditMode::Tempo.is_tempo());
        assert!(!EditMode::Bend.is_tempo());
        assert!(!EditMode::Cc(0).is_tempo());
    }

    #[test]
    fn test_edit_mode_all_modes() {
        let modes = EditMode::all_modes();
        assert_eq!(modes.len(), 1);
        assert_eq!(modes[0], EditMode::Velocity);
    }

    #[test]
    fn test_cc_point_construction() {
        let point = CcPoint {
            tick: 480.0,
            value: 64,
        };
        assert_eq!(point.tick, 480.0);
        assert_eq!(point.value, 64);
    }

    #[test]
    fn test_bend_point_construction() {
        let point = BendPoint {
            tick: 960.0,
            value: 0,
        };
        assert_eq!(point.tick, 960.0);
        assert_eq!(point.value, 0);
        let p_neg = BendPoint {
            tick: 0.0,
            value: -8192,
        };
        assert_eq!(p_neg.value, -8192);
        let p_pos = BendPoint {
            tick: 0.0,
            value: 8191,
        };
        assert_eq!(p_pos.value, 8191);
    }

    #[test]
    fn test_velocity_point_construction() {
        let point = VelocityPoint {
            note_index: 5,
            tick: 100.0,
            velocity: 80,
            length: 480.0,
        };
        assert_eq!(point.note_index, 5);
        assert_eq!(point.tick, 100.0);
        assert_eq!(point.velocity, 80);
    }

    #[test]
    fn test_tempo_point_construction() {
        let tempo_pt = TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        };
        assert_eq!(tempo_pt.tick, 0.0);
        assert_eq!(tempo_pt.bpm, 120.0);
    }

    #[test]
    fn test_cc_display_known_controller() {
        let display = CcDisplay(7);
        let display_str = display.to_string();
        assert!(display_str.contains("7"));
        assert!(display_str.contains("Volume"));
    }

    #[test]
    fn test_cc_display_unknown_controller() {
        let display = CcDisplay(255);
        let display_str = display.to_string();
        assert_eq!(display_str, "255");
    }

    #[test]
    fn test_bend_display() {
        let display_str = BendDisplay.to_string();
        assert!(display_str.contains("Bend"));
        assert!(display_str.contains("-8192"));
    }

    #[test]
    fn test_cc_data_default() {
        let data = CcData::default();
        assert!(data.controllers.is_empty());
        assert!(data.bend_points.is_empty());
    }

    #[test]
    fn test_cc_controller_names_contains_known() {
        assert!(CC_CONTROLLER_NAMES.iter().any(|(n, _)| *n == 7));
        assert!(CC_CONTROLLER_NAMES.iter().any(|(n, _)| *n == 64));
        assert!(CC_CONTROLLER_NAMES.iter().any(|(n, _)| *n == 120));
    }
}
