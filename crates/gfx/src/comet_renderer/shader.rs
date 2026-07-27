//! Comet WGSL 着色器组装
//!
//! wgpu 不原生支持 `#include`，因此我们在 Rust 侧将公共代码与各个样式入口
//! 拼接成一个完整的 WGSL 模块。

use super::CometRenderStyle;

/// 公共 WGSL 代码（数据结构、Bindings、工具函数）。
const COMMON: &str = include_str!("../shaders/comet_common.wgsl");

/// 各样式的 WGSL 入口代码。
const ENHANCED: &str = include_str!("../shaders/comet_enhanced.wgsl");
const MIDITRAIL: &str = include_str!("../shaders/comet_miditrail.wgsl");
const PFA: &str = include_str!("../shaders/comet_pfa.wgsl");
const VELOCITIES: &str = include_str!("../shaders/comet_velocities.wgsl");
const CHANNELS: &str = include_str!("../shaders/comet_channels.wgsl");

/// 获取指定样式的完整 WGSL 源码。
pub fn source_for_style(style: CometRenderStyle) -> String {
    let style_source = match style {
        CometRenderStyle::Enhanced => ENHANCED,
        CometRenderStyle::MIDITrail => MIDITRAIL,
        CometRenderStyle::PFA => PFA,
        CometRenderStyle::Velocities => VELOCITIES,
        CometRenderStyle::Channels => CHANNELS,
    };
    format!("{}\n{}", COMMON, style_source)
}
