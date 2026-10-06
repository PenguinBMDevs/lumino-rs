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

use std::collections::VecDeque;
use std::sync::Arc;

use lumino_midi_model::multi_port::PercussionTracker;
use xsynth_core::{
    AudioPipe, AudioStreamParams,
    channel_group::{ChannelGroup, SynthEvent},
};

use super::{
    config::AudioRenderConfig, limiter::AudioLimiter, stream::SampleSink, tick_conv::TickToTime,
};

/// 诊断探针：最近派发事件（首个非有限样本出现时随日志导出）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct RecentEvent {
    /// 事件派发时的累计渲染帧（换算秒定位 NaN 时刻）
    pub(crate) frame: u64,
    /// 事件类型：0=NoteOn 1=NoteOff 2=CC 3=PC 4=PB
    pub(crate) code: u8,
    pub(crate) port: u8,
    pub(crate) channel: u8,
    pub(crate) a: u16,
    pub(crate) b: u16,
}

/// 事件处理器的合成后端抽象（REND-010 #102 ①）。
///
/// 生产实现是 xsynth [`ChannelGroup`]（`send_event` 派发 + `AudioPipe` 渲染）；
/// 测试注入记录型桩即可对 `dispatch_event` 的**事件级端口映射**做端到端断言，
/// 无需真实音色库/音频渲染。调用频率为每事件/每批一次（非每样本），
/// `dyn` 分发开销可忽略。
pub(crate) trait SynthBackend {
    /// 音频流参数（采样率 / 通道数）。
    fn stream_params(&self) -> &AudioStreamParams;
    /// 派发合成事件。
    fn send_event(&mut self, event: SynthEvent);
    /// 渲染音频样本到缓冲（测试桩可只填静音）。
    fn read_samples_unchecked(&mut self, buffer: &mut [f32]);
}

impl SynthBackend for ChannelGroup {
    fn stream_params(&self) -> &AudioStreamParams {
        AudioPipe::stream_params(self)
    }

    fn send_event(&mut self, event: SynthEvent) {
        ChannelGroup::send_event(self, event);
    }

    fn read_samples_unchecked(&mut self, buffer: &mut [f32]) {
        AudioPipe::read_samples_unchecked(self, buffer)
    }
}

/// 事件处理器 — 将 MIDI 事件流式渲染到 SampleSink
///
/// 参考 OmniConverter 的 EventsProcesser 设计：
/// - 以渲染时间（delta seconds）驱动，而非依赖外部定时器
/// - 使用 Vec 回收池减少分配
pub(crate) struct MidiEventProcessor<'a> {
    config: &'a AudioRenderConfig,
    channel_group: &'a mut dyn SynthBackend,
    tick_conv: &'a mut TickToTime,
    sink: &'a mut dyn SampleSink,
    sample_rate: u32,
    channel_count: u16,
    /// Vec 回收池
    vec_pool: Vec<Vec<f32>>,
    /// 限幅器（启用时）
    limiter: Option<AudioLimiter>,
    /// REND-002 方案 B：运行时通道「音符/打击乐」模态跟踪（Bank Select 约定）。
    percussion: PercussionTracker,
    /// 诊断：最近 64 条派发事件（NaN 取证，只保留一次）
    recent_events: VecDeque<RecentEvent>,
    /// 诊断：首个非有限样本是否已记录
    nan_probe_done: bool,
    /// 已写出到 sink 的累计帧数（诊断定位用）
    frames_rendered: u64,
    /// 非有限样本总数（已按静音净化；导出结束汇总告警）
    non_finite_total: u64,
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
