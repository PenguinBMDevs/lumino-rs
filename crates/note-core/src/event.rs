//! 事件浏览器数据模型
//!
//! 定义时间签名、调号、标记、歌词、和弦、音色变换以及自动化事件等
//! 供事件浏览器展示与编辑的数据类型。

/// 拍号事件
#[derive(Debug, Clone, PartialEq)]
pub struct TimeSignatureEvent {
    /// 事件所在 tick
    pub tick: u32,
    /// 拍号分子
    pub numerator: u8,
    /// 拍号分母（人类可读值，如 4、8）
    pub denominator: u8,
}

/// 调号事件
#[derive(Debug, Clone, PartialEq)]
pub struct KeySignatureEvent {
    /// 事件所在 tick
    pub tick: u32,
    /// 根音（0-11，C=0）
    pub root: u8,
    /// 调式
    pub scale: ScaleType,
}

/// 标记事件
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerEvent {
    /// 事件所在 tick
    pub tick: u32,
    /// 标记文本
    pub text: String,
}

/// 歌词事件
#[derive(Debug, Clone, PartialEq)]
pub struct LyricsEvent {
    /// 所属音轨（0 = Conductor）
    pub track: u16,
    /// 事件所在 tick
    pub tick: u32,
    /// 歌词文本
    pub text: String,
}

/// 和弦事件
#[derive(Debug, Clone, PartialEq)]
pub struct ChordEvent {
    /// 所属音轨（0 = Conductor）
    pub track: u16,
    /// 事件所在 tick
    pub tick: u32,
    /// 和弦文本
    pub text: String,
}

/// 音色变换事件
#[derive(Debug, Clone, PartialEq)]
pub struct ProgramChangeEvent {
    /// 所属音轨（0 = Conductor）
    pub track: u16,
    /// 事件所在 tick
    pub tick: u32,
    /// 音色编号
    pub program: u8,
}

/// 自动化事件
#[derive(Debug, Clone, PartialEq)]
pub struct AutomationEvent {
    /// 事件所在 tick
    pub tick: u32,
    /// 事件值（范围取决于目标）
    pub value: f32,
    /// 线段插值形状
    pub shape: SegmentShape,
}

/// 自动化目标
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutomationTarget {
    /// MIDI CC（0-127）
    Cc(u8),
    /// 弯音（0-16383）
    PitchBend,
    /// RPN（0-16383）
    Rpn(u16),
    /// NRPN（0-16383）
    Nrpn(u16),
    /// 速度
    Tempo,
}

impl AutomationTarget {
    /// 该目标的最大原始值
    pub fn max_value(&self) -> f32 {
        match self {
            AutomationTarget::Cc(_) => 127.0,
            AutomationTarget::PitchBend => 16383.0,
            AutomationTarget::Rpn(_) => 16383.0,
            AutomationTarget::Nrpn(_) => 16383.0,
            AutomationTarget::Tempo => 999.0,
        }
    }
}

/// 调式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScaleType {
    /// 大调
    Major,
    /// 自然小调
    Minor,
    /// 多利亚
    Dorian,
    /// 弗里几亚
    Phrygian,
    /// 利底亚
    Lydian,
    /// 混合利底亚
    Mixolydian,
    /// 爱奥利亚
    Aeolian,
    /// 洛克利亚
    Locrian,
    /// 和声小调
    HarmonicMinor,
    /// 旋律小调
    MelodicMinor,
}

impl ScaleType {
    /// 所有支持的调式列表
    pub const ALL: &'static [Self] = &[
        ScaleType::Major,
        ScaleType::Minor,
        ScaleType::Dorian,
        ScaleType::Phrygian,
        ScaleType::Lydian,
        ScaleType::Mixolydian,
        ScaleType::Aeolian,
        ScaleType::Locrian,
        ScaleType::HarmonicMinor,
        ScaleType::MelodicMinor,
    ];
}

/// 自动化线段插值形状
#[derive(Debug, Clone, Copy)]
pub enum SegmentShape {
    /// 保持当前值直到下一事件
    Step,
    /// 贝塞尔曲线控制点
    Curve {
        /// 第一个控制点 x
        x1: f32,
        /// 第一个控制点 y
        y1: f32,
        /// 第二个控制点 x
        x2: f32,
        /// 第二个控制点 y
        y2: f32,
    },
}

impl PartialEq for SegmentShape {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (SegmentShape::Step, SegmentShape::Step) => true,
            (
                SegmentShape::Curve {
                    x1: a1,
                    y1: b1,
                    x2: c1,
                    y2: d1,
                },
                SegmentShape::Curve {
                    x1: a2,
                    y1: b2,
                    x2: c2,
                    y2: d2,
                },
            ) => {
                a1.to_bits() == a2.to_bits()
                    && b1.to_bits() == b2.to_bits()
                    && c1.to_bits() == c2.to_bits()
                    && d1.to_bits() == d2.to_bits()
            }
            _ => false,
        }
    }
}

impl Eq for SegmentShape {}

impl std::hash::Hash for SegmentShape {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            SegmentShape::Step => 0u8.hash(state),
            SegmentShape::Curve { x1, y1, x2, y2 } => {
                1u8.hash(state);
                x1.to_bits().hash(state);
                y1.to_bits().hash(state);
                x2.to_bits().hash(state);
                y2.to_bits().hash(state);
            }
        }
    }
}
