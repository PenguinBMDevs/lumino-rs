//! 格式化函数 — 按语言格式化音符精度、符点类型、配置枚举等显示名称

use lumino_core::types::Language;
use lumino_core::{DotType, NotePrecision};

/// 获取音符精度名称（按语言）
pub fn note_precision_name(precision: NotePrecision, lang: Language) -> &'static str {
    match lang {
        Language::ZhCn => match precision {
            NotePrecision::Whole => "全音符",
            NotePrecision::Half => "二分音符",
            NotePrecision::Quarter => "四分音符",
            NotePrecision::Eighth => "八分音符",
            NotePrecision::Sixteenth => "十六分音符",
            NotePrecision::ThirtySecond => "三十二分音符",
            NotePrecision::SixtyFourth => "六十四分音符",
            NotePrecision::OneTwentyEighth => "128分音符",
            NotePrecision::Custom => "自定义",
        },
        Language::EnUs => match precision {
            NotePrecision::Whole => "Whole Note",
            NotePrecision::Half => "Half Note",
            NotePrecision::Quarter => "Quarter Note",
            NotePrecision::Eighth => "Eighth Note",
            NotePrecision::Sixteenth => "Sixteenth Note",
            NotePrecision::ThirtySecond => "32nd Note",
            NotePrecision::SixtyFourth => "64th Note",
            NotePrecision::OneTwentyEighth => "128th Note",
            NotePrecision::Custom => "Custom",
        },
    }
}

/// 获取符点类型名称（按语言）
pub fn dot_type_name(dot_type: DotType, lang: Language) -> &'static str {
    use lumino_core::DotType::*;
    match lang {
        Language::ZhCn => match dot_type {
            None => "（无）",
            Tuplet => "连音符",
            Single => "符点",
            Double => "双符点",
        },
        Language::EnUs => match dot_type {
            None => "(None)",
            Tuplet => "Tuplet",
            Single => "Dotted",
            Double => "Double Dotted",
        },
    }
}

/// 获取框选框模式显示名称（按语言）
pub fn selection_box_mode_name(
    mode: lumino_core::storage::config::SelectionBoxMode,
    lang: Language,
) -> &'static str {
    use lumino_core::storage::config::SelectionBoxMode::*;
    match lang {
        Language::ZhCn => match mode {
            Direct => "直接跟随",
            Spring => "弹簧动画",
        },
        Language::EnUs => match mode {
            Direct => "Direct",
            Spring => "Spring Animation",
        },
    }
}

/// 获取橡皮擦行为显示名称（按语言）
pub fn eraser_behavior_name(
    behavior: lumino_core::storage::config::EraserBehavior,
    lang: Language,
) -> &'static str {
    use lumino_core::storage::config::EraserBehavior::*;
    match lang {
        Language::ZhCn => match behavior {
            Default => "默认 (Shift+拖动框选)",
            DirectSelect => "直接框选 (无需Shift)",
        },
        Language::EnUs => match behavior {
            Default => "Default (Shift+drag)",
            DirectSelect => "Direct Select (no Shift)",
        },
    }
}

/// 获取音轨添加行为显示名称（按语言）
pub fn track_add_behavior_name(
    behavior: lumino_core::storage::config::TrackAddBehavior,
    lang: Language,
) -> &'static str {
    use lumino_core::storage::config::TrackAddBehavior::*;
    match lang {
        Language::ZhCn => match behavior {
            AutoSwitch => "自动跳转到新音轨",
            StayCurrent => "保持当前音轨",
        },
        Language::EnUs => match behavior {
            AutoSwitch => "Auto-switch to new track",
            StayCurrent => "Stay on current track",
        },
    }
}

/// 获取合成器后端显示名称（按语言）
pub fn synth_backend_name(
    backend: lumino_core::storage::config::SynthBackend,
    lang: Language,
) -> &'static str {
    use lumino_core::storage::config::SynthBackend::*;
    match lang {
        Language::ZhCn => match backend {
            XSynth => "XSynth (内置)",
            Kdmapi => "KDMAPI",
            System => "系统 MIDI",
        },
        Language::EnUs => match backend {
            XSynth => "XSynth (Built-in)",
            Kdmapi => "KDMAPI",
            System => "System MIDI",
        },
    }
}
