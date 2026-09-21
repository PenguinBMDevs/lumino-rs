//! 设置页面 - 音频设置本地化类型
//!
//! 由 `pages/audio.rs` 原样拆分，仅调整可见性，逻辑零变更。

use lumino_core::storage::config::SynthBackend;
use lumino_extras::i18n::Language;
use lumino_ui_core::settings_event::OutputType;

/// 本地化输出类型（顶层 MIDI 输出类型）
#[derive(Debug, Clone, Copy)]
pub(super) struct LocalizedOutputType {
    pub(super) inner: OutputType,
    name: &'static str,
}

impl PartialEq for LocalizedOutputType {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LocalizedOutputType {}

impl std::fmt::Display for LocalizedOutputType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl LocalizedOutputType {
    pub(super) fn new(ot: OutputType, lang: Language) -> Self {
        let name = match ot {
            OutputType::Builtin => match lang {
                Language::ZhCn => "内置合成器",
                Language::EnUs => "Built-in Synth",
            },
            OutputType::Kdmapi => "KDMAPI",
            OutputType::System => match lang {
                Language::ZhCn => "系统 MIDI",
                Language::EnUs => "System MIDI",
            },
        };
        Self { inner: ot, name }
    }
}

/// 本地化内置合成器引擎（内置类型下的子下拉，与 xsynth-realtime 共用同一列表）
#[derive(Debug, Clone, Copy)]
pub(super) struct LocalizedBuiltinEngine {
    pub(super) inner: SynthBackend,
    name: &'static str,
}

impl PartialEq for LocalizedBuiltinEngine {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for LocalizedBuiltinEngine {}

impl std::fmt::Display for LocalizedBuiltinEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl LocalizedBuiltinEngine {
    pub(super) fn new(b: SynthBackend, lang: Language) -> Self {
        let name = match b {
            SynthBackend::XSynth => match lang {
                Language::ZhCn => "XSynth (Realtime)",
                Language::EnUs => "XSynth (Realtime)",
            },
            SynthBackend::Lgs => "LGS (GPU)",
            _ => "Unknown",
        };
        Self { inner: b, name }
    }
}
