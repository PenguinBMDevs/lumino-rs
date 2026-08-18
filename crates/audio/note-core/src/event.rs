//! 自动化事件数据模型
//!
//! 定义自动化事件、自动化目标与线段插值形状等数据类型。
//!
//! 2026-08 事件浏览器类型清理：拍号/调号/标记/歌词/和弦/音色变换的强类型
//! 事件（TimeSignatureEvent / KeySignatureEvent / MarkerEvent / LyricsEvent /
//! ChordEvent / ProgramChangeEvent）随 EditorData 孤儿字段删除后全库零使用，
//! 对应数据以原始格式存储于 MidiDocument / LuminoProject，此处不再定义。

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
