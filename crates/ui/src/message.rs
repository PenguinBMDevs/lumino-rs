//! 应用消息 — 重新导出自 lumino-message
//!
//! 将泛型 Message<W, S, Se, T> 实例化为具体的 UI 事件类型，
//! 保持与原有 `crate::message::*` 路径完全兼容。

// 重新导出 UI 特有的事件类型（这些不能放入 lumino-message）
pub use crate::{
    settings::Event as Settings, sidebar::Event as Sidebar, toolbar::Event as Toolbar,
    window::Event as Window,
};

// 重新导出自 lumino-message 的所有公共类型
pub use lumino_message::{
    AudioAction,
    // 共享类型
    AudioChannels,
    AudioExportAction,
    AudioFormat,
    CcOption,
    DotType,
    EditorAction,
    Interpolation,
    LoopRangeAction,
    Message as GenericMessage,
    NotePrecision,
    PatternAction,
    PerfData,
    SpeedChangeAction,
    SpeedFactor,
    ThreadingOption,
    Tool,
    TupletType,
    VelocityAction,
};

/// 具体化的消息类型
///
/// 将泛型 Message 绑定到 UI crate 的具体事件类型。
pub type Message = GenericMessage<Window, Sidebar, Settings, Toolbar>;

/// 创建空消息
pub const fn null() -> Message {
    Message::Null
}
