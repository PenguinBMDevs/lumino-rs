//! 音符精度/网格对齐设置

use crate::Renderer;
use crate::{Element, Message, Theme, window};
use iced_widget::container;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotePrecision {
    Whole,
    Half,
    #[default]
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
    OneTwentyEighth,
    Custom,
}

impl std::fmt::Display for NotePrecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            NotePrecision::Whole => "全音符",
            NotePrecision::Half => "二分音符",
            NotePrecision::Quarter => "四分音符",
            NotePrecision::Eighth => "八分音符",
            NotePrecision::Sixteenth => "十六分音符",
            NotePrecision::ThirtySecond => "三十二分音符",
            NotePrecision::SixtyFourth => "六十四分音符",
            NotePrecision::OneTwentyEighth => "128分音符",
            NotePrecision::Custom => "自定义",
        };
        write!(f, "{}", name)
    }
}

impl NotePrecision {
    pub fn display_name(&self) -> &'static str {
        match self {
            NotePrecision::Whole => "全音符",
            NotePrecision::Half => "二分音符",
            NotePrecision::Quarter => "四分音符",
            NotePrecision::Eighth => "八分音符",
            NotePrecision::Sixteenth => "十六分音符",
            NotePrecision::ThirtySecond => "三十二分音符",
            NotePrecision::SixtyFourth => "六十四分音符",
            NotePrecision::OneTwentyEighth => "128分音符",
            NotePrecision::Custom => "自定义",
        }
    }

    pub fn as_ticks(self, ppq: u16) -> f32 {
        let ppq = ppq as f32;
        match self {
            NotePrecision::Whole => ppq * 4.0,
            NotePrecision::Half => ppq * 2.0,
            NotePrecision::Quarter => ppq,
            NotePrecision::Eighth => ppq / 2.0,
            NotePrecision::Sixteenth => ppq / 4.0,
            NotePrecision::ThirtySecond => ppq / 8.0,
            NotePrecision::SixtyFourth => ppq / 16.0,
            NotePrecision::OneTwentyEighth => ppq / 32.0,
            NotePrecision::Custom => ppq / 4.0,
        }
    }

    pub fn presets() -> &'static [NotePrecision] {
        &[
            NotePrecision::Whole,
            NotePrecision::Half,
            NotePrecision::Quarter,
            NotePrecision::Eighth,
            NotePrecision::Sixteenth,
            NotePrecision::ThirtySecond,
            NotePrecision::SixtyFourth,
            NotePrecision::OneTwentyEighth,
        ]
    }
}
