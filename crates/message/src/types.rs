//! lumino-message 内部共享类型
//!
//! 这些类型原本分散在 lumino-ui 的各个模块中（state::root_state, toolbar::types,
//! editor::velocity, statusbar::performance），因为被 Message 枚举引用而被提取到此处。
//! 跨 crate 共享的领域类型（AudioAction, DotType, NotePrecision, Tool）位于
//! `lumino-core`，请通过 `lumino_message::*` 的 re-export 使用。

// ─── 性能监控数据 ───

/// 性能监控数据
#[derive(Debug, Clone, Copy, Default)]
pub struct PerfData {
    /// 当前 FPS
    pub fps: f32,
    /// CPU 使用率百分比（0.0 ~ 100.0，100% = 所有核心满载）
    pub cpu_usage: f32,
    /// 进程内存占用（MB）
    pub memory_mb: f32,
    /// GPU 帧耗时（ms）
    pub gpu_frame_time_ms: f32,
}

impl PerfData {
    pub fn new(fps: f32, cpu_usage: f32, memory_mb: f32, gpu_frame_time_ms: f32) -> Self {
        Self {
            fps,
            cpu_usage,
            memory_mb,
            gpu_frame_time_ms,
        }
    }
}

// ─── 三连音类型 ───

/// 三连音类型选项
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TupletType {
    /// 普通（无）
    #[default]
    None,
    /// 三连音
    Triplet,
    /// 五连音
    Quintuplet,
    /// 六连音
    Sextuplet,
    /// 七连音
    Septuplet,
}

impl std::fmt::Display for TupletType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            TupletType::None => "（无）",
            TupletType::Triplet => "3",
            TupletType::Quintuplet => "5",
            TupletType::Sextuplet => "6",
            TupletType::Septuplet => "7",
        };
        write!(f, "{}", name)
    }
}

impl TupletType {
    /// 获取所有选项
    pub fn all() -> &'static [TupletType] {
        &[
            TupletType::None,
            TupletType::Triplet,
            TupletType::Quintuplet,
            TupletType::Sextuplet,
            TupletType::Septuplet,
        ]
    }

    /// 获取数值
    pub fn value(&self) -> u32 {
        match self {
            TupletType::None => 1,
            TupletType::Triplet => 3,
            TupletType::Quintuplet => 5,
            TupletType::Sextuplet => 6,
            TupletType::Septuplet => 7,
        }
    }
}

// ─── 音符变速速度因子 ───

/// 音符变速速度因子预设
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub enum SpeedFactor {
    /// 4 倍速
    X4,
    /// 2 倍速
    X2,
    /// 1 倍速（原始长度）
    X1,
    /// 0.5 倍速
    #[default]
    X05,
    /// 0.25 倍速
    X025,
}

impl SpeedFactor {
    /// 获取所有预设值
    pub fn all() -> &'static [SpeedFactor] {
        &[
            SpeedFactor::X025,
            SpeedFactor::X05,
            SpeedFactor::X1,
            SpeedFactor::X2,
            SpeedFactor::X4,
        ]
    }

    /// 获取倍数值
    pub fn value(self) -> f32 {
        match self {
            SpeedFactor::X025 => 0.25,
            SpeedFactor::X05 => 0.5,
            SpeedFactor::X1 => 1.0,
            SpeedFactor::X2 => 2.0,
            SpeedFactor::X4 => 4.0,
        }
    }

    /// 获取显示名称
    pub fn display_name(self) -> &'static str {
        match self {
            SpeedFactor::X025 => "×0.25",
            SpeedFactor::X05 => "×0.5",
            SpeedFactor::X1 => "×1.0",
            SpeedFactor::X2 => "×2.0",
            SpeedFactor::X4 => "×4.0",
        }
    }
}

impl std::fmt::Display for SpeedFactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ─── 音频导出相关类型 ───

/// 音频通道数（UI用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioChannels {
    Mono,
    #[default]
    Stereo,
}

impl std::fmt::Display for AudioChannels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioChannels::Mono => write!(f, "单声道"),
            AudioChannels::Stereo => write!(f, "立体声"),
        }
    }
}

/// 多线程选项（UI用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadingOption {
    None,
    #[default]
    Auto,
    Manual(u32),
}

impl std::fmt::Display for ThreadingOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThreadingOption::None => write!(f, "关闭"),
            ThreadingOption::Auto => write!(f, "自动"),
            ThreadingOption::Manual(n) => write!(f, "{} 线程", n),
        }
    }
}

/// 插值算法（UI用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    None,
    #[default]
    Linear,
}

impl std::fmt::Display for Interpolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Interpolation::None => write!(f, "无插值"),
            Interpolation::Linear => write!(f, "线性插值"),
        }
    }
}

/// 音频格式（UI用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioFormat {
    #[default]
    WAV,
    FLAC,
}

impl std::fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioFormat::WAV => write!(f, "WAV"),
            AudioFormat::FLAC => write!(f, "FLAC"),
        }
    }
}

// ─── CC 或 Bend 下拉选项 ───

/// CC 或 Bend 下拉选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CcOption {
    /// 弯音
    Bend,
    /// CC 控制器
    Cc(u8),
}

impl std::fmt::Display for CcOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CcOption::Bend => write!(f, "Bend: Pitch Bend (-8192..8191)"),
            CcOption::Cc(n) => match CC_CONTROLLER_NAMES.iter().find(|(num, _)| *num == *n) {
                Some((_, name)) => write!(f, "{}: {}", n, name),
                None => write!(f, "{}", n),
            },
        }
    }
}

/// CC 控制器名称映射
pub const CC_CONTROLLER_NAMES: &[(u8, &str)] = &[
    (0, "Bank Select"),
    (1, "Modulation"),
    (2, "Breath Controller"),
    (3, "Undefined"),
    (4, "Foot Controller"),
    (5, "Portamento Time"),
    (6, "Data Entry MSB"),
    (7, "Volume"),
    (8, "Balance"),
    (9, "Undefined"),
    (10, "Pan"),
    (11, "Expression"),
    (12, "Effect Control 1"),
    (13, "Effect Control 2"),
    (14, "Undefined"),
    (15, "Undefined"),
    (16, "General Purpose 1"),
    (17, "General Purpose 2"),
    (18, "General Purpose 3"),
    (19, "General Purpose 4"),
    (20, "Undefined"),
    (21, "Undefined"),
    (22, "Undefined"),
    (23, "Undefined"),
    (24, "Undefined"),
    (25, "Undefined"),
    (26, "Undefined"),
    (27, "Undefined"),
    (28, "Undefined"),
    (29, "Undefined"),
    (30, "Undefined"),
    (31, "Undefined"),
    (32, "Bank Select LSB"),
    (33, "Modulation LSB"),
    (34, "Breath Controller LSB"),
    (35, "Undefined"),
    (36, "Foot Controller LSB"),
    (37, "Portamento Time LSB"),
    (38, "Data Entry LSB"),
    (39, "Volume LSB"),
    (40, "Balance LSB"),
    (41, "Undefined"),
    (42, "Pan LSB"),
    (43, "Expression LSB"),
    (44, "Effect Control 1 LSB"),
    (45, "Effect Control 2 LSB"),
    (46, "Undefined"),
    (47, "Undefined"),
    (48, "General Purpose 1 LSB"),
    (49, "General Purpose 2 LSB"),
    (50, "General Purpose 3 LSB"),
    (51, "General Purpose 4 LSB"),
    (52, "Undefined"),
    (53, "Undefined"),
    (54, "Undefined"),
    (55, "Undefined"),
    (56, "Undefined"),
    (57, "Undefined"),
    (58, "Undefined"),
    (59, "Undefined"),
    (60, "Undefined"),
    (61, "Undefined"),
    (62, "Undefined"),
    (63, "Undefined"),
    (64, "Sustain Pedal"),
    (65, "Portamento"),
    (66, "Sostenuto"),
    (67, "Soft Pedal"),
    (68, "Legato Footswitch"),
    (69, "Hold 2"),
    (70, "Sound Controller 1"),
    (71, "Sound Controller 2"),
    (72, "Sound Controller 3"),
    (73, "Sound Controller 4"),
    (74, "Sound Controller 5"),
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
    (85, "Undefined"),
    (86, "Undefined"),
    (87, "Undefined"),
    (88, "Undefined"),
    (89, "Undefined"),
    (90, "Undefined"),
    (91, "Effects 1 Depth"),
    (92, "Effects 2 Depth"),
    (93, "Effects 3 Depth"),
    (94, "Effects 4 Depth"),
    (95, "Effects 5 Depth"),
    (96, "Data Increment"),
    (97, "Data Decrement"),
    (98, "NRPN LSB"),
    (99, "NRPN MSB"),
    (100, "RPN LSB"),
    (101, "RPN MSB"),
    (102, "Undefined"),
    (103, "Undefined"),
    (104, "Undefined"),
    (105, "Undefined"),
    (106, "Undefined"),
    (107, "Undefined"),
    (108, "Undefined"),
    (109, "Undefined"),
    (110, "Undefined"),
    (111, "Undefined"),
    (112, "Undefined"),
    (113, "Undefined"),
    (114, "Undefined"),
    (115, "Undefined"),
    (116, "Undefined"),
    (117, "Undefined"),
    (118, "Undefined"),
    (119, "Undefined"),
    (120, "All Sound Off"),
    (121, "Reset All Controllers"),
    (122, "Local Control"),
    (123, "All Notes Off"),
    (124, "Omni Mode Off"),
    (125, "Omni Mode On"),
    (126, "Mono Mode"),
    (127, "Poly Mode"),
];
