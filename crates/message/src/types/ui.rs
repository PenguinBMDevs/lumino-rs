//! UI 相关类型

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
