//! Toolbar 类型定义子模块

/// 音符精度/网格对齐设置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotePrecision {
    /// 全音符 (4拍)
    Whole,
    /// 二分音符 (2拍)
    Half,
    /// 四分音符 (1拍)
    #[default]
    Quarter,
    /// 八分音符 (1/2拍)
    Eighth,
    /// 十六分音符 (1/4拍)
    Sixteenth,
    /// 三十二分音符 (1/8拍)
    ThirtySecond,
    /// 六十四分音符 (1/16拍)
    SixtyFourth,
    /// 128分音符 (1/32拍)
    OneTwentyEighth,
    /// 自定义
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
    /// 获取精度显示名称
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

    /// 根据PPQ计算对应的tick值
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
            NotePrecision::Custom => ppq / 4.0, // 默认自定义为十六分音符
        }
    }

    /// 获取所有预设选项（不包括自定义）
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

/// 符点类型选项
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DotType {
    /// 无符点
    #[default]
    None,
    /// 连音符
    Tuplet,
    /// 单符点
    Single,
    /// 双符点
    Double,
}

impl std::fmt::Display for DotType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            DotType::None => "（无）",
            DotType::Tuplet => "连音符",
            DotType::Single => "符点",
            DotType::Double => "双符点",
        };
        write!(f, "{}", name)
    }
}

impl DotType {
    /// 获取所有选项
    pub fn all() -> &'static [DotType] {
        &[
            DotType::None,
            DotType::Tuplet,
            DotType::Single,
            DotType::Double,
        ]
    }

    /// 获取倍数（符点增加原时值的多少）
    pub fn multiplier(&self) -> f32 {
        match self {
            DotType::None => 1.0,
            DotType::Tuplet => 1.0,
            DotType::Single => 1.5,
            DotType::Double => 1.75,
        }
    }
}

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

/// 自定义精度对话框状态
#[derive(Debug, Clone)]
pub struct CustomPrecisionDialog {
    pub is_open: bool,
    /// 三连音数量（如 "3"）
    pub tuplet_count: String,
    /// 三连音类型
    pub tuplet_type: TupletType,
    /// 符点类型
    pub dot_type: DotType,
    /// 分音符值（如 "64"）
    pub note_value: String,
    /// 除数（如 "1"）
    pub divisor: String,
}

impl Default for CustomPrecisionDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            tuplet_count: "3".to_string(),
            tuplet_type: TupletType::Triplet,
            dot_type: DotType::None,
            note_value: "64".to_string(),
            divisor: "1".to_string(),
        }
    }
}

impl CustomPrecisionDialog {
    /// 计算对应的tick值（基于PPQ）
    pub fn calculate_ticks(&self, ppq: u16) -> Option<f32> {
        let note_value = self.note_value.parse::<f32>().ok()?;
        let divisor = self.divisor.parse::<f32>().ok()?;

        if note_value == 0.0 || divisor == 0.0 {
            return None;
        }

        // 基础时值 = (4 / 分音符值) * PPQ
        // 例如 64分音符 = (4 / 64) * PPQ = PPQ / 16
        let base_ticks = (ppq as f32) * 4.0 / note_value;

        // 应用连音：只有当符点类型不是"（无）"时才使用三连音数量
        // 连音将N个音符塞进N-1个的时值，比例 = (N-1) / N
        let tuplet_ratio = if self.dot_type != DotType::None {
            if let Ok(tuplet_count) = self.tuplet_count.parse::<f32>() {
                if tuplet_count > 1.0 {
                    (tuplet_count - 1.0) / tuplet_count
                } else {
                    1.0
                }
            } else {
                1.0
            }
        } else {
            1.0
        };

        // 应用符点
        let dot_multiplier = self.dot_type.multiplier();

        // 应用除数
        let final_ticks = base_ticks * tuplet_ratio * dot_multiplier / divisor;

        Some(final_ticks)
    }

    /// 获取显示文本
    pub fn display_text(&self) -> String {
        let mut text = String::new();
        if self.tuplet_count != "1" && !self.tuplet_count.is_empty() {
            text.push_str(&self.tuplet_count);
            text.push(' ');
        }
        text.push_str(&self.note_value);
        text.push_str("分音符");
        if self.divisor != "1" && !self.divisor.is_empty() {
            text.push_str(" / ");
            text.push_str(&self.divisor);
        }
        text
    }
}

/// 工具类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Pointer,
    Pencil,
    Brush,
    Pen,
    Eraser,
    Razor,
}

/// 工具栏默认高度
pub const DEFAULT_HEIGHT: f32 = 72.0;
/// 最小高度
pub const MIN_HEIGHT: f32 = 56.0;
/// 最大高度
pub const MAX_HEIGHT: f32 = 200.0;
/// 拖拽手柄高度
pub const RESIZE_HANDLE_HEIGHT: f32 = 6.0;
