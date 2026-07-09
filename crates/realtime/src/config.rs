//! 配置选项

use xsynth_core::channel::ChannelInitOptions;
use xsynth_core::channel_group::ThreadCount;

/// 合成器格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthFormat {
    /// 标准 MIDI 格式（16通道）
    Midi,
    /// 自定义通道数
    Custom { channels: u32 },
}

/// 实时合成器配置
#[derive(Debug, Clone)]
pub struct XSynthRealtimeConfig {
    /// 渲染窗口大小（毫秒）
    pub render_window_ms: f64,

    /// 多线程配置
    pub multithreading: ThreadCount,

    /// 合成器格式
    pub format: SynthFormat,

    /// 通道初始化选项
    pub channel_init_options: ChannelInitOptions,

    /// 最大每秒事件数（None = 不限制）
    pub max_nps: Option<u64>,

    /// 渲染警告阈值（毫秒），超过此值会输出警告
    pub render_warn_threshold_ms: f64,

    /// 忽略的事件范围
    pub ignore_range: Option<std::ops::Range<u8>>,
}

impl Default for XSynthRealtimeConfig {
    fn default() -> Self {
        Self {
            render_window_ms: 10.0,
            multithreading: ThreadCount::Auto,
            format: SynthFormat::Midi,
            channel_init_options: ChannelInitOptions::default(),
            max_nps: None,
            render_warn_threshold_ms: 20.0,
            ignore_range: None,
        }
    }
}
