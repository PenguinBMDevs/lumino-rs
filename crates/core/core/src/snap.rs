//! 吸附规格（DRAW-037）
//!
//! 统一“栏/音符/附点/三连音/自定义”的吸附口径。音符值以 4 = 四分音符为基准；
//! 所有 tick 计算走整数有理数，避免 1/3 累积误差。
//!
//! `Custom { ticks }` 是旧 `set_snap_precision(777.0)` 的兼容路径，不出现在
//! 选择器里；选择器只展示 [`SnapSpec::presets`] 中的规格。

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::types::NotePrecision;

/// 吸附规格
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SnapSpec {
    /// 小节：钢琴卷帘按 4/4 的 `ppq*4`，工程走带按当前拍号段的小节
    Bar,
    /// 音符：`notes` 以 4 = 四分音符；`dots` 0/1/2；`div` 1 = 不分割，3 = 三连音
    Note {
        /// 音符分母（1/2/4/8/16/32/64/128）
        notes: u8,
        /// 附点数（0/1/2）
        dots: u8,
        /// 分割数（1..=64；3 = 三连音）
        div: u8,
    },
    /// 绝对 tick（旧 `set_snap_precision` 兼容）
    Custom {
        /// tick 数（至少 1）
        ticks: u32,
    },
}

/// 吸附规格校验错误
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SnapError {
    /// 音符值非法
    #[error("音符值必须是 1/2/4/8/16/32/64/128 之一（实际 {0}）")]
    Notes(u8),
    /// 附点数非法
    #[error("附点数只能是 0/1/2（实际 {0}）")]
    Dots(u8),
    /// 分割数非法
    #[error("分割数必须在 1..=64（实际 {0}）")]
    Div(u8),
    /// 绝对 tick 非法
    #[error("绝对 tick 必须大于 0")]
    Ticks,
}

impl Default for SnapSpec {
    fn default() -> Self {
        Self::QUARTER
    }
}

impl SnapSpec {
    /// 四分音符
    pub const QUARTER: Self = Self::Note {
        notes: 4,
        dots: 0,
        div: 1,
    };

    /// 构造并校验音符规格
    pub fn note(notes: u8, dots: u8, div: u8) -> Result<Self, SnapError> {
        if !matches!(notes, 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128) {
            return Err(SnapError::Notes(notes));
        }
        if dots > 2 {
            return Err(SnapError::Dots(dots));
        }
        if !(1..=64).contains(&div) {
            return Err(SnapError::Div(div));
        }
        Ok(Self::Note { notes, dots, div })
    }

    /// 构造绝对 tick 规格
    pub fn custom(ticks: u32) -> Result<Self, SnapError> {
        if ticks == 0 {
            return Err(SnapError::Ticks);
        }
        Ok(Self::Custom { ticks })
    }

    /// 校验规格
    pub fn validate(self) -> Result<(), SnapError> {
        match self {
            Self::Bar => Ok(()),
            Self::Note { notes, dots, div } => Self::note(notes, dots, div).map(|_| ()),
            Self::Custom { ticks } => Self::custom(ticks).map(|_| ()),
        }
    }

    /// 钢琴卷帘/4-4 口径下的吸附 tick
    pub fn ticks(self, ppq: u16) -> u32 {
        match self {
            Self::Bar => u32::from(ppq.max(1)) * 4,
            Self::Custom { ticks } => ticks.max(1),
            Self::Note { notes, dots, div } => {
                // ppq*4 * (2^(dots+1)-1) / (notes * 2^dots * div)，四舍五入
                let ppq = u64::from(ppq.max(1));
                let notes = u64::from(notes.max(1));
                let dots = u32::from(dots.min(2));
                let div = u64::from(div.max(1));
                let dot_num = (1_u64 << (dots + 1)) - 1;
                let dot_den = 1_u64 << dots;
                // div=3 是“三连音”：3 个音占 2 个基准音，因子 2/3；
                // div=1 不分割；div>1 统一用 2/div（三连音/五连音等）。
                let (div_num, div_den) = if div > 1 { (2, div) } else { (1, 1) };
                let num = ppq * 4 * dot_num * div_num;
                let den = notes * dot_den * div_den;
                let ticks = (num + den / 2) / den;
                u32::try_from(ticks.max(1)).unwrap_or(u32::MAX)
            }
        }
    }

    /// 工程走带口径下的吸附 tick
    ///
    /// `Bar` 用当前拍号段的真实小节；其余规格沿用既有“按拍长比例”的行为
    /// （Quarter = 1 拍、Eighth = 1/2 拍），保证旧吸附结果等价。
    pub fn ticks_in_measure(self, ppq: u16, numerator: u8, denominator: u8) -> u32 {
        let ppq = u64::from(ppq.max(1));
        let numerator = u64::from(numerator.max(1));
        let denominator = u64::from(denominator.max(1));
        let beat_num = ppq * 4;
        match self {
            Self::Bar => {
                let measure_num = beat_num * numerator;
                let ticks = (measure_num + denominator / 2) / denominator;
                u32::try_from(ticks.max(1)).unwrap_or(u32::MAX)
            }
            Self::Custom { ticks } => ticks.max(1),
            Self::Note { notes, dots, div } => {
                let notes = u64::from(notes.max(1));
                let dots = u32::from(dots.min(2));
                let div = u64::from(div.max(1));
                let dot_num = (1_u64 << (dots + 1)) - 1;
                let dot_den = 1_u64 << dots;
                // beat * (4 / notes) * dot * (2/div)
                let (div_num, div_den) = if div > 1 { (2, div) } else { (1, 1) };
                let num = beat_num * 4 * dot_num * div_num;
                let den = denominator * notes * dot_den * div_den;
                let ticks = (num + den / 2) / den;
                u32::try_from(ticks.max(1)).unwrap_or(u32::MAX)
            }
        }
    }

    /// 显示文本（选择器/按钮）
    pub fn text(self) -> String {
        match self {
            Self::Bar => "Bar".to_string(),
            Self::Custom { ticks } => format!("Custom {ticks}"),
            Self::Note { notes, dots, div } => {
                let mut text = format!("1/{notes}");
                for _ in 0..dots {
                    text.push('.');
                }
                if div != 1 {
                    text.push(' ');
                    text.push_str(&div.to_string());
                }
                text
            }
        }
    }

    /// 选择器预设（不含 Custom 绝对 tick）
    pub fn presets() -> Vec<Self> {
        let mut list = vec![Self::Bar];
        for notes in [1, 2, 4, 8, 16, 32, 64, 128] {
            list.push(Self::Note {
                notes,
                dots: 0,
                div: 1,
            });
        }
        for notes in [2, 4, 8] {
            list.push(Self::Note {
                notes,
                dots: 1,
                div: 1,
            });
        }
        for notes in [4, 8, 16] {
            list.push(Self::Note {
                notes,
                dots: 0,
                div: 3,
            });
        }
        list
    }

    /// 旧 `NotePrecision` → `SnapSpec`
    ///
    /// `Whole` 映射为 Bar（工程走带旧语义就是小节）；`Custom` 使用调用方提供的
    /// 绝对 tick（没有则退回四分音符）。
    pub fn from_legacy(precision: NotePrecision, custom_ticks: Option<u32>) -> Self {
        match precision {
            NotePrecision::Whole => Self::Bar,
            NotePrecision::Half => Self::Note {
                notes: 2,
                dots: 0,
                div: 1,
            },
            NotePrecision::Quarter => Self::QUARTER,
            NotePrecision::Eighth => Self::Note {
                notes: 8,
                dots: 0,
                div: 1,
            },
            NotePrecision::Sixteenth => Self::Note {
                notes: 16,
                dots: 0,
                div: 1,
            },
            NotePrecision::ThirtySecond => Self::Note {
                notes: 32,
                dots: 0,
                div: 1,
            },
            NotePrecision::SixtyFourth => Self::Note {
                notes: 64,
                dots: 0,
                div: 1,
            },
            NotePrecision::OneTwentyEighth => Self::Note {
                notes: 128,
                dots: 0,
                div: 1,
            },
            NotePrecision::Custom => Self::Custom {
                ticks: custom_ticks.unwrap_or(1).max(1),
            },
        }
    }
}

/// 便捷函数：钢琴卷帘/4-4 口径
pub fn snap_ticks(spec: SnapSpec, ppq: u16) -> u32 {
    spec.ticks(ppq)
}

/// 便捷函数：工程走带拍号口径
pub fn snap_ticks_in_measure(spec: SnapSpec, ppq: u16, numerator: u8, denominator: u8) -> u32 {
    spec.ticks_in_measure(ppq, numerator, denominator)
}

/// 便捷函数：显示文本
pub fn snap_text(spec: SnapSpec) -> String {
    spec.text()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_ticks_480_960() {
        assert_eq!(SnapSpec::Bar.ticks(480), 1920);
        assert_eq!(SnapSpec::Bar.ticks(960), 3840);
        assert_eq!(SnapSpec::QUARTER.ticks(480), 480);
        assert_eq!(SnapSpec::QUARTER.ticks(960), 960);
        assert_eq!(
            SnapSpec::Note {
                notes: 8,
                dots: 0,
                div: 1
            }
            .ticks(480),
            240
        );
        assert_eq!(
            SnapSpec::Note {
                notes: 4,
                dots: 1,
                div: 1
            }
            .ticks(480),
            720
        );
        assert_eq!(
            SnapSpec::Note {
                notes: 4,
                dots: 2,
                div: 1
            }
            .ticks(480),
            840
        );
        assert_eq!(
            SnapSpec::Note {
                notes: 4,
                dots: 0,
                div: 3
            }
            .ticks(480),
            320
        );
        assert_eq!(
            SnapSpec::Note {
                notes: 8,
                dots: 0,
                div: 3
            }
            .ticks(480),
            160
        );
    }

    #[test]
    fn test_measure_ticks_follow_time_signature() {
        assert_eq!(SnapSpec::Bar.ticks_in_measure(480, 4, 4), 1920);
        assert_eq!(SnapSpec::Bar.ticks_in_measure(480, 3, 4), 1440);
        assert_eq!(SnapSpec::Bar.ticks_in_measure(480, 6, 8), 1440);
        assert_eq!(SnapSpec::QUARTER.ticks_in_measure(480, 6, 8), 240);
        assert_eq!(
            SnapSpec::Note {
                notes: 8,
                dots: 0,
                div: 1
            }
            .ticks_in_measure(480, 6, 8),
            120
        );
    }

    #[test]
    fn test_validation_and_text() {
        assert!(SnapSpec::note(3, 0, 1).is_err());
        assert!(SnapSpec::note(4, 3, 1).is_err());
        assert!(SnapSpec::note(4, 0, 0).is_err());
        assert_eq!(SnapSpec::note(4, 1, 1).expect("合法").text(), "1/4.");
        assert_eq!(SnapSpec::note(4, 2, 1).expect("合法").text(), "1/4..");
        assert_eq!(SnapSpec::note(4, 0, 3).expect("合法").text(), "1/4 3");
        assert_eq!(SnapSpec::Bar.text(), "Bar");
        assert!(SnapSpec::presets().contains(&SnapSpec::Bar));
    }

    #[test]
    fn test_legacy_mapping() {
        assert_eq!(
            SnapSpec::from_legacy(NotePrecision::Whole, None),
            SnapSpec::Bar
        );
        assert_eq!(
            SnapSpec::from_legacy(NotePrecision::Quarter, None),
            SnapSpec::QUARTER
        );
        assert_eq!(
            SnapSpec::from_legacy(NotePrecision::Custom, Some(777)),
            SnapSpec::Custom { ticks: 777 }
        );
    }
}
