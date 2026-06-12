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
    pub tick: f32,
    pub bpm: f64,
}
