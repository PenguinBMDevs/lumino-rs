//! lumino-core 共享领域类型
//!
//! 这些类型不依赖 UI 或消息系统，被 `lumino-core` 自身以及 `lumino-message`
//! 共同使用。将它们放在 `core` 中，避免 `lumino-core` 反向依赖 `lumino-message`。

// ─── 音频动作 ───

/// 音频动作
///
/// 表示播放或停止单个音符的指令，用于 MIDI 预览和实时演奏。
#[derive(Debug, Clone)]
pub enum AudioAction {
    /// 以指定力度播放音符
    PlayNote { key: u8, velocity: u8 },
    /// 停止指定音符
    StopNote { key: u8 },
}

// ─── 工具类型 ───

/// 工具类型
///
/// 表示编辑器当前激活的交互工具。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    /// 指针/选择工具
    #[default]
    Pointer,
    /// Y 向框选工具（Y 维度自动全选，X 维度按音符精度 snap 框选）
    PointerYSelect,
    /// 铅笔工具（绘制音符 / 自动化点）
    Pencil,
    /// 画笔工具
    Brush,
    /// 钢笔工具
    Pen,
    /// 橡皮擦工具
    Eraser,
    /// 切割工具
    Razor,
    /// 曲线工具（自动化曲线绘制）
    Curve,
}

// ─── 音符精度/网格对齐 ───

/// 音符精度/网格对齐设置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotePrecision {
    /// 全音符 (4拍)
    #[default]
    Whole,
    /// 二分音符 (2拍)
    Half,
    /// 四分音符 (1拍)
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

    /// 根据 PPQ 计算对应的 tick 值
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

// ─── 符点类型 ───

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

// ─── 语言支持 ───

/// 支持的语言
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum Language {
    /// 简体中文（默认）
    #[serde(rename = "zh-CN")]
    #[default]
    ZhCn,
    /// English
    #[serde(rename = "en-US")]
    EnUs,
}

impl Language {
    /// 返回所有可用语言列表
    pub fn all() -> [Language; 2] {
        [Language::ZhCn, Language::EnUs]
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::ZhCn => write!(f, "简体中文"),
            Language::EnUs => write!(f, "English"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── AudioAction ───

    #[test]
    fn test_audio_action_play_note() {
        let action = AudioAction::PlayNote {
            key: 60,
            velocity: 100,
        };
        assert!(matches!(action, AudioAction::PlayNote { key: 60, .. }));
    }

    #[test]
    fn test_audio_action_stop_note() {
        let action = AudioAction::StopNote { key: 60 };
        assert!(matches!(action, AudioAction::StopNote { key: 60 }));
    }

    // ─── Tool ───

    #[test]
    fn test_tool_default() {
        assert_eq!(Tool::default(), Tool::Pointer);
    }

    // ─── NotePrecision ───

    #[test]
    fn test_note_precision_default() {
        assert_eq!(NotePrecision::default(), NotePrecision::Whole);
    }

    #[test]
    fn test_note_precision_display() {
        assert_eq!(NotePrecision::Whole.to_string(), "全音符");
        assert_eq!(NotePrecision::Quarter.to_string(), "四分音符");
        assert_eq!(NotePrecision::Custom.to_string(), "自定义");
    }

    #[test]
    fn test_note_precision_as_ticks() {
        let ppq = 480;
        assert_eq!(NotePrecision::Whole.as_ticks(ppq), 480.0 * 4.0);
        assert_eq!(NotePrecision::Quarter.as_ticks(ppq), 480.0);
        assert_eq!(NotePrecision::Eighth.as_ticks(ppq), 480.0 / 2.0);
        assert_eq!(NotePrecision::OneTwentyEighth.as_ticks(ppq), 480.0 / 32.0);
    }

    #[test]
    fn test_note_precision_presets() {
        let presets = NotePrecision::presets();
        assert_eq!(presets.len(), 8);
        assert!(!presets.contains(&NotePrecision::Custom));
    }

    // ─── DotType ───

    #[test]
    fn test_dot_type_default() {
        assert_eq!(DotType::default(), DotType::None);
    }

    #[test]
    fn test_dot_type_multiplier() {
        assert_eq!(DotType::None.multiplier(), 1.0);
        assert_eq!(DotType::Single.multiplier(), 1.5);
        assert_eq!(DotType::Double.multiplier(), 1.75);
    }
}
