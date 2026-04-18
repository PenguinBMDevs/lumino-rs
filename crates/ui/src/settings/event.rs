//! 设置面板事件枚举

use lumino_core::storage::config::SynthBackend;

#[derive(Debug, Clone)]
pub enum Event {
    MenuSelected(usize),
    SynthBackendChanged(SynthBackend),
    SoundfontPathChanged(String),
    BrowseSoundfont,
    NativeTitlebarChanged(bool),
    XSynthBufferChanged(f64),
    XSynthSampleRateChanged(u32),
    XSynthThreadsChanged(i32),
    XSynthFadeOutChanged(bool),
    ThemeChanged(String),
    EraserBehaviorChanged(lumino_core::storage::config::EraserBehavior),
    ProgramFontNameChanged(String),
    ProgramFontPathChanged(String),
    BrowseProgramFont,
    AutoScrollFixedPositionChanged(String),
    AutoScrollPageTriggerOffsetChanged(String),
    AutoScrollPageReturnPositionChanged(String),
    VelocityFilterThresholdChanged(String),
}
