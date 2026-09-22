//! MIDI 事件处理器 — 参考 OmniConverter 的 EventsProcesser
//!
//! 将 MIDI 事件流转换为 xsynth 音频样本。
//! 基于渲染时间驱动，支持进度回调。

// ── 子模块（按职责拆分，零逻辑变更）──────────────────────────────
// - `processor`: MidiEventProcessor 的事件分发 / 弯音归一化 / 尾部收尾
// - `render`: 批量渲染（帧域）与 Vec 缓冲池
// - `soundfont`: 音色库路径校验与加载
mod processor;
mod render;
mod soundfont;

pub use soundfont::load_soundfonts;

use std::sync::Arc;

use xsynth_core::channel_group::ChannelGroup;

use super::{
    config::AudioRenderConfig, limiter::AudioLimiter, stream::SampleSink, tick_conv::TickToTime,
};

/// 事件处理器 — 将 MIDI 事件流式渲染到 SampleSink
///
/// 参考 OmniConverter 的 EventsProcesser 设计：
/// - 以渲染时间（delta seconds）驱动，而非依赖外部定时器
/// - 使用 Vec 回收池减少分配
pub struct MidiEventProcessor<'a> {
    config: &'a AudioRenderConfig,
    channel_group: &'a mut ChannelGroup,
    tick_conv: &'a mut TickToTime,
    sink: &'a mut dyn SampleSink,
    sample_rate: u32,
    channel_count: u16,
    /// Vec 回收池
    vec_pool: Vec<Vec<f32>>,
    /// 限幅器（启用时）
    limiter: Option<AudioLimiter>,
}

/// 进度回调
pub type ProgressFn = Arc<dyn Fn(String, f64) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::processor::pitch_bend_normalized;
    use midly::PitchBend;

    /// 回归：`PitchBend::as_int()` 是带符号偏移，中心必须映射到 0。
    /// 旧实现 `as_int()/8192 - 1` 把中心（raw 0x2000）算成 -1.0（全下弯音）。
    #[test]
    fn pitch_bend_normalization_uses_signed_offset() {
        assert!(pitch_bend_normalized(PitchBend::mid_raw_value()).abs() < 1e-6);
        assert!((pitch_bend_normalized(PitchBend::min_raw_value()) + 1.0).abs() < 1e-6);
        assert!((pitch_bend_normalized(PitchBend::max_raw_value()) - 8191.0 / 8192.0).abs() < 1e-6);
        // 半程上弯（+4096）→ +0.5，旧实现会得到 -0.5。
        assert!((pitch_bend_normalized(PitchBend::from_int(4096)) - 0.5).abs() < 1e-6);
    }
}
