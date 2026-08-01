//! Settings 事件子模块
//!
//! 设置面板的事件枚举（仅枚举，不包含面板逻辑）。

use lumino_core::storage::config::{
    EraserBehavior, SelectionBoxMode, SynthBackend, TrackAddBehavior, TrackDisplayMode,
};
use lumino_extras::i18n::Language;

/// 设置面板事件
#[derive(Debug, Clone)]
pub enum Event {
    MenuSelected(usize),
    SynthBackendChanged(SynthBackend),
    SoundfontPathChanged(String),
    BrowseSoundfont,
    NativeTitlebarChanged(bool),
    XSynthBufferChanged(f64),
    XSynthSampleRateChanged(u32),
    XSynthFadeOutChanged(bool),
    XSynthMaxVoicesChanged(Option<usize>),
    ThemeChanged(String),
    EraserBehaviorChanged(EraserBehavior),
    SelectionBoxModeChanged(SelectionBoxMode),
    ProgramFontNameChanged(String),
    ProgramFontPathChanged(String),
    BrowseProgramFont,
    AutoScrollFixedPositionChanged(String),
    AutoScrollPageTriggerOffsetChanged(String),
    AutoScrollPageReturnPositionChanged(String),
    VelocityFilterThresholdChanged(String),
    IconHiDPIChanged(bool),
    Enable256keyChanged(bool),
    VelocityCurveStyleChanged(bool),
    DeviceSelected(u32),
    LanguageChanged(Language),
    HiresOnionEnabledChanged(bool),
    HiresMeasuresPerGroupChanged(String),
    HiresTileWidthChanged(String),
    HiresCooldownChanged(String),
    HiresGpuMemLimitChanged(String),
    PlaybackKeyColorsEnabledChanged(bool),
    TrackAddBehaviorChanged(TrackAddBehavior),
    PaletteChanged(String),
    TrackDisplayModeChanged(TrackDisplayMode),
    // 编辑设置
    HistoryTotalLimitChanged(String),
    HistoryEntryLimitChanged(String),
    MergeWindowMsChanged(String),
    InterceptNotificationChanged(bool),
    /// 自动化曲线连线粗细（像素，1-10）
    AutomationLineThicknessChanged(f32),
}
