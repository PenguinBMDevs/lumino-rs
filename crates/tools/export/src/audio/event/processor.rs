//! `MidiEventProcessor` 实现 — 事件分发、渲染驱动与限幅兜底
//!
//! 自 `event.rs` 拆分而来（零逻辑变更）。

use midly::{MidiMessage, PitchBend, TrackEventKind};
use tracing::info;
use xsynth_core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
    channel_group::{SynthEvent, SynthFormat},
};

use lumino_midi_model::multi_port::{PercussionTracker, track_global_channel};

use crate::audio::{
    config::AudioRenderConfig, limiter::AudioLimiter, stream::SampleSink, tick_conv::TickToTime,
};
use crate::error::ExportResult;

use super::{MidiEventProcessor, SynthBackend};

/// MIDI 弯音事件 → xsynth 归一化值（-1.0..1.0）。
///
/// `lumino_midly::PitchBend::as_int()` 返回的是**带符号偏移**（-8192..=8191，
/// 中心 0），不是 raw 14-bit（0..16383）；直接除 8192 即可。旧实现
/// `as_int()/8192 - 1` 把中心值算成 -1.0（全下弯音），导致 CPU 渲染里所有
/// 中心 PB 事件都把音高拉低一个灵敏度值。
pub(super) fn pitch_bend_normalized(bend: PitchBend) -> f32 {
    bend.as_int() as f32 / 8192.0
}

/// 事件通道 → 合成层全局通道（REND-002）。
///
/// - 单端口（`config.midi_max_port == 0`）：恒等映射（`channel` 直通），保持历史行为；
/// - 多端口：`(effective_port(port), channel) → port*16 + channel`，超产品上限的
///   端口折叠到端口 15 块（B1 决策，告警由渲染入口负责）。
#[inline]
pub(super) fn global_event_channel(config: &AudioRenderConfig, port: u8, channel: u8) -> u32 {
    if config.midi_max_port == 0 {
        u32::from(channel)
    } else {
        u32::from(track_global_channel(port, channel))
    }
}

/// 尾部静音判定阈值：样本绝对值低于此值视为静音。
const TAIL_SILENCE_EPS: f32 = 0.0001;

/// 非有限样本（NaN/Inf）按静音净化。
///
/// 返回 `(净化数量, 首个非有限样本偏移)`。
///
/// **渲染主循环与尾部收尾必须共用本函数**：两处口径分叉正是 REND-002 尾部
/// NaN 旁路的根因——尾部直接写 sink，漏净化时限幅器关闭会把 NaN/Inf 直写 WAV；
/// 且 `is_silent` 的 `abs() < eps` 判据对 NaN 恒为 false，污染尾部会让收尾
/// 循环跑满批次上限（120s 垃圾）。
pub(super) fn purify_non_finite(buffer: &mut [f32]) -> (u64, Option<usize>) {
    let mut bad = 0_u64;
    let mut first_bad: Option<usize> = None;
    for (i, sample) in buffer.iter_mut().enumerate() {
        if !sample.is_finite() {
            if first_bad.is_none() {
                first_bad = Some(i);
            }
            *sample = 0.0;
            bad += 1;
        }
    }
    (bad, first_bad)
}

impl<'a> MidiEventProcessor<'a> {
    /// 创建 MIDI 事件处理器。
    ///
    /// 将 MIDI 事件按时间轴播放到给定的合成通道组与采样输出。
    pub fn new(
        config: &'a AudioRenderConfig,
        channel_group: &'a mut dyn SynthBackend,
        tick_conv: &'a mut TickToTime,
        sink: &'a mut dyn SampleSink,
    ) -> Self {
        let params = *channel_group.stream_params();
        let limiter = if config.apply_limiter {
            Some(AudioLimiter::new(
                params.sample_rate,
                params.channels.count(),
                0.95,
            ))
        } else {
            None
        };
        // REND-002：模态跟踪按合成层实际通道数初始化（与 `synth_format` 一致）。
        let synth_channels = match config.synth_format() {
            SynthFormat::Midi => 16,
            SynthFormat::Custom { channels } => channels,
        };
        MidiEventProcessor {
            config,
            channel_group,
            tick_conv,
            sink,
            sample_rate: params.sample_rate,
            channel_count: params.channels.count(),
            vec_pool: Vec::new(),
            limiter,
            percussion: PercussionTracker::new(synth_channels),
            recent_events: std::collections::VecDeque::with_capacity(64),
            nan_probe_done: false,
            frames_rendered: 0,
            non_finite_total: 0,
        }
    }

    /// NaN 诊断：记录最近派发事件（容量 64，环形丢弃最旧）。
    fn push_recent_event(&mut self, port: u8, channel: u8, message: &MidiMessage) {
        let (code, a, b) = match message {
            MidiMessage::NoteOn { key, vel, .. } => (0, u16::from(*key), u16::from(vel.as_int())),
            MidiMessage::NoteOff { key, .. } => (1, u16::from(*key), 0),
            MidiMessage::Controller { controller, value } => {
                (2, u16::from(controller.as_int()), u16::from(value.as_int()))
            }
            MidiMessage::ProgramChange { program } => (3, u16::from(program.as_int()), 0),
            MidiMessage::PitchBend { bend } => (4, bend.as_int() as u16, 0),
            MidiMessage::Aftertouch { key, vel } => {
                (5, u16::from(key.as_int()), u16::from(vel.as_int()))
            }
            MidiMessage::ChannelAftertouch { vel } => (6, u16::from(vel.as_int()), 0),
        };
        if self.recent_events.len() == 64 {
            self.recent_events.pop_front();
        }
        self.recent_events.push_back(super::RecentEvent {
            frame: self.frames_rendered,
            code,
            port,
            channel,
            a,
            b,
        });
    }

    /// 渲染批次写出后累计帧数（诊断定位用）。
    pub(crate) fn add_rendered_frames(&mut self, frames: u64) {
        self.frames_rendered = self.frames_rendered.saturating_add(frames);
    }

    /// 非有限样本计数累加（已按静音净化）。
    pub(crate) fn add_non_finite(&mut self, count: u64) {
        self.non_finite_total = self.non_finite_total.saturating_add(count);
    }

    /// 非有限样本总数。
    pub(crate) fn non_finite_total(&self) -> u64 {
        self.non_finite_total
    }

    /// NaN 诊断：首个非有限样本的取证日志（一次性）。
    pub(crate) fn probe_non_finite(&mut self, offset: usize) {
        if self.nan_probe_done {
            return;
        }
        self.nan_probe_done = true;
        let frame_size = usize::from(self.channel_count.max(1));
        let nan_frame = self.frames_rendered + (offset / frame_size) as u64;
        let sr = f64::from(self.sample_rate.max(1));
        let dump: Vec<String> = self
            .recent_events
            .iter()
            .map(|e| {
                format!(
                    "t{:.3}s k{} p{} c{} a{} b{}",
                    e.frame as f64 / sr,
                    e.code,
                    e.port,
                    e.channel,
                    e.a,
                    e.b
                )
            })
            .collect();
        // 诊断场景必须可见：不依赖 tracing subscriber（示例/无头导出可能未初始化）。
        eprintln!(
            "[REND-002][NaN-PROBE] 首个非有限样本: t={:.3}s frame={nan_frame} ch={}; 最近事件(code:0=On 1=Off 2=CC 3=PC 4=PB 5=AT 6=CAT): {:?}",
            nan_frame as f64 / sr,
            offset % frame_size,
            dump
        );
    }

    /// 事件 tick → 目标帧（向下取整）。
    ///
    /// 走 [`TickToTime::seconds_at`] 游标（O(1) 摊销），在**整数帧域**累积，
    /// 修掉旧实现逐事件 `(delta 秒 × sr) as usize` 截断带来的亚采样时基漂移。
    pub(crate) fn frame_at_tick(&mut self, tick: u64) -> u64 {
        let secs = self.tick_conv.seconds_at(tick);
        (secs * f64::from(self.sample_rate)) as u64
    }

    /// 判断音符是否应被过滤（力度/键位）
    #[inline]
    fn is_note_filtered(&self, key: u8, velocity: u8) -> bool {
        if self.config.filter_key && (key < self.config.key_low || key > self.config.key_high) {
            return true;
        }
        if self.config.filter_velocity
            && (velocity < self.config.velocity_low || velocity > self.config.velocity_high)
        {
            return true;
        }
        false
    }

    /// 投递一个 MIDI 事件（不推进时间；时间推进由渲染循环按块调度）。
    ///
    /// `port` 为该事件来源轨道的 MIDI 端口（FF 21）。单端口导出
    /// （`config.midi_max_port == 0`）下忽略端口、保持恒等映射；
    /// 多端口下按 `(port, ch) → port*16 + ch` 映射到全局通道。
    ///
    /// 返回因 `note_force_end_delay` 额外渲染的帧数（无则 0）——调用方必须把它
    /// 计入采样时钟，避免后续重复渲染。
    pub(crate) fn dispatch_event(
        &mut self,
        event_kind: &TrackEventKind,
        port: u8,
    ) -> ExportResult<u64> {
        if let Some(ctrl) = &self.config.control {
            ctrl.wait_if_paused();
            ctrl.check_abort()?;
        }
        let mut extra_frames = 0_u64;

        // 发送 MIDI 事件到合成器
        if let TrackEventKind::Midi { channel, message } = event_kind {
            // REND-002：单端口（midi_max_port==0）保持 `channel.as_int()` 恒等路径，
            // 与历史行为完全一致；多端口才启用全局通道映射。
            let ch = global_event_channel(self.config, port, channel.as_int());
            self.push_recent_event(port, ch as u8, message);
            let force_end_frames = self.force_end_delay_frames();
            match message {
                MidiMessage::NoteOn { key, vel } => {
                    let vel_u8 = vel.as_int();
                    // velocity 0 的 NoteOn 按 MIDI 规范视为 NoteOff
                    if vel_u8 == 0 {
                        if self.config.filter_key
                            && (*key < self.config.key_low || *key > self.config.key_high)
                        {
                            return Ok(extra_frames);
                        }
                        if force_end_frames > 0 {
                            self.render_frames(force_end_frames)?;
                            extra_frames += force_end_frames;
                        }
                        self.channel_group.send_event(SynthEvent::Channel(
                            ch,
                            ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: *key }),
                        ));
                        return Ok(extra_frames);
                    }
                    if self.is_note_filtered(*key, vel_u8) {
                        return Ok(extra_frames);
                    }
                    self.channel_group.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                            key: *key,
                            vel: vel_u8,
                        }),
                    ));
                }
                MidiMessage::NoteOff { key, .. } => {
                    if self.config.filter_key
                        && (*key < self.config.key_low || *key > self.config.key_high)
                    {
                        return Ok(extra_frames);
                    }
                    // note_force_end_delay：延长音符，延迟发送 NoteOff
                    if force_end_frames > 0 {
                        self.render_frames(force_end_frames)?;
                        extra_frames += force_end_frames;
                    }
                    self.channel_group.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: *key }),
                    ));
                }
                MidiMessage::Controller { controller, value } => {
                    // REND-002 方案 B：Bank Select（CC0/CC32）驱动的运行时打击乐
                    // 模态切换。xsynth 在打击乐模态下忽略 CC0，因此切换必须在
                    // 转发本条 CC 之前显式下发；CC32 只在收到过 CC0 后才参与判定。
                    if let Some(on) =
                        self.percussion
                            .observe_cc(ch as u16, controller.as_int(), value.as_int())
                    {
                        self.channel_group.send_event(SynthEvent::Channel(
                            ch,
                            ChannelEvent::Config(ChannelConfigEvent::SetPercussionMode(on)),
                        ));
                    }
                    self.channel_group.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                            controller.as_int(),
                            value.as_int(),
                        ))),
                    ));
                }
                MidiMessage::ProgramChange { program } => {
                    if self.config.ignore_program_changes {
                        return Ok(extra_frames);
                    }
                    self.channel_group.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(program.as_int())),
                    ));
                }
                MidiMessage::PitchBend { bend } => {
                    self.channel_group.send_event(SynthEvent::Channel(
                        ch,
                        ChannelEvent::Audio(ChannelAudioEvent::Control(
                            ControlEvent::PitchBendValue(pitch_bend_normalized(*bend)),
                        )),
                    ));
                }
                _ => {}
            }
        }

        Ok(extra_frames)
    }

    /// `note_force_end_delay` 对应的帧数（毫秒 → 帧，向下取整；与旧实现同口径）。
    fn force_end_delay_frames(&self) -> u64 {
        u64::from(self.config.note_force_end_delay) * u64::from(self.sample_rate) / 1000
    }

    /// 尾部单批处理：**净化 → 限幅 → 静音判定 → 写 sink → 帧数累计**。
    ///
    /// 返回该批是否静音（调用方据此收尾）。净化与 `render_frames` 同序同源
    /// （共用 [`purify_non_finite`]），保证尾部不再有 NaN 旁路。
    ///
    /// 单独成函数是为了让「尾部也必须净化」成为可测断言：收尾循环读到的样本
    /// 由合成器决定、测试无法注入，而本函数接受调用方给定的缓冲。
    fn write_tail_batch(&mut self, buffer: &mut [f32]) -> ExportResult<bool> {
        let (bad, first_bad) = purify_non_finite(buffer);
        if let Some(offset) = first_bad {
            self.probe_non_finite(offset);
        }
        if bad > 0 {
            self.add_non_finite(bad);
        }

        if let Some(limiter) = self.limiter.as_mut() {
            limiter.process(buffer);
        }

        // 净化在前，故此处 NaN 已归零 → 污染尾部也能正常判静音收尾（不再跑满上限）。
        let is_silent = buffer.iter().all(|&s| s.abs() < TAIL_SILENCE_EPS);

        self.sink.write_samples(buffer)?;
        // 帧数累计：保持 `frames_rendered` 与探针报告的绝对帧号一致。
        let channels = usize::from(self.channel_count).max(1);
        let frames = (buffer.len() / channels) as u64;
        self.add_rendered_frames(frames);
        Ok(is_silent)
    }

    /// 完成渲染：发送 NoteOff，渲染尾部直到静音
    pub fn finalize(&mut self) -> ExportResult<()> {
        if let Some(ctrl) = &self.config.control {
            ctrl.wait_if_paused();
            ctrl.check_abort()?;
        }
        // 发送所有音符关闭
        self.channel_group
            .send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
                ChannelAudioEvent::AllNotesOff,
            )));
        self.channel_group
            .send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
                ChannelAudioEvent::ResetControl,
            )));

        // 持续渲染尾部直到静音（带 120s 安全上限，防止无限循环）
        let frame_size = self.channel_count as usize;
        let batch_size = self.sample_rate as usize * frame_size; // 1秒
        let max_batches = 120; // 120 秒上限，对齐 GPU 侧 max_tail_seconds

        for _ in 0..max_batches {
            if let Some(ctrl) = &self.config.control {
                ctrl.wait_if_paused();
                ctrl.check_abort()?;
            }
            let mut buffer = vec![0.0f32; batch_size];
            self.channel_group.read_samples_unchecked(&mut buffer);

            // REND-002 修复：尾部同样走净化（此前尾部直写 sink，是唯一的 NaN 旁路）。
            if self.write_tail_batch(&mut buffer)? {
                break;
            }
        }

        let bad = self.non_finite_total();
        if bad > 0 {
            tracing::warn!(
                "[REND-002] 导出期间检测到 {bad} 个非有限样本（NaN/Inf），已按静音净化；\
                 这通常意味着上游合成数值污染（请附带素材/音色库上报定位）"
            );
        }

        info!("音频渲染完成");
        Ok(())
    }
}

/// 简单的限幅器（兜底，已被 AudioLimiter 替代，保留用于独立调用）
#[allow(dead_code)]
fn apply_limiter(samples: &mut [f32], _channels: u16) {
    // 简单的峰值限制
    let threshold = 0.95;
    for sample in samples.iter_mut() {
        if sample.abs() > threshold {
            *sample = sample.signum() * threshold;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单端口：恒等映射，且**忽略**传入端口（门控关闭，零行为变化）。
    #[test]
    fn single_port_is_identity_and_ignores_port() {
        let config = AudioRenderConfig::default();
        for ch in 0u8..16 {
            assert_eq!(global_event_channel(&config, 0, ch), u32::from(ch));
        }
        assert_eq!(
            global_event_channel(&config, 7, 5),
            5u32,
            "单端口下端口必须被忽略（门控关闭）"
        );
    }

    /// 多端口：`port*16+ch`，端口 0 恒等，端口间互不重叠。
    #[test]
    fn multi_port_maps_to_port_blocks() {
        let config = AudioRenderConfig {
            midi_max_port: 6,
            ..Default::default()
        };
        assert_eq!(global_event_channel(&config, 0, 5), 5);
        assert_eq!(global_event_channel(&config, 1, 5), 21, "端口 1 ch5 → 21");
        assert_eq!(global_event_channel(&config, 1, 9), 25, "端口 1 ch9 → 25");
        assert_eq!(global_event_channel(&config, 6, 15), 111, "7 端口边界");
    }

    /// 超产品上限端口折叠到端口 15 块（B1），不丢事件。
    #[test]
    fn over_limit_port_folds_to_last_block() {
        let config = AudioRenderConfig {
            midi_max_port: 127,
            ..Default::default()
        };
        assert_eq!(
            global_event_channel(&config, 127, 3),
            15 * 16 + 3,
            "超上限端口应折叠到端口 15 块"
        );
        assert_eq!(global_event_channel(&config, 16, 9), 15 * 16 + 9);
    }

    // ── #102 ①：事件级端口映射 e2e（记录型合成桩）──────────────────

    use crate::audio::stream::VecSampleSink;
    use crate::audio::tick_conv::TickToTime;
    use midly::num::{u4, u7};
    use xsynth_core::{AudioStreamParams, ChannelCount};

    /// 记录型合成桩：收集派发事件，渲染输出静音。
    struct RecordingSink {
        events: Vec<SynthEvent>,
        params: AudioStreamParams,
    }

    impl RecordingSink {
        fn new(sample_rate: u32) -> Self {
            Self {
                events: Vec::new(),
                params: AudioStreamParams::new(sample_rate, ChannelCount::Stereo),
            }
        }

        /// 仅 `SynthEvent::Channel` 的全局通道序列（事件级映射断言用）。
        fn event_channels(&self) -> Vec<u32> {
            self.events
                .iter()
                .filter_map(|event| match event {
                    SynthEvent::Channel(ch, _) => Some(*ch),
                    SynthEvent::AllChannels(_) => None,
                })
                .collect()
        }
    }

    impl SynthBackend for RecordingSink {
        fn stream_params(&self) -> &AudioStreamParams {
            &self.params
        }

        fn send_event(&mut self, event: SynthEvent) {
            self.events.push(event);
        }

        fn read_samples_unchecked(&mut self, buffer: &mut [f32]) {
            buffer.fill(0.0);
        }
    }

    /// 构造指定 MIDI 通道的 NoteOn（力度 100）。
    fn note_on(channel: u8, key: u8) -> TrackEventKind<'static> {
        TrackEventKind::Midi {
            channel: u4::from(channel),
            message: MidiMessage::NoteOn {
                key,
                vel: u7::from(100),
            },
        }
    }

    /// #102 ①：多端口事件级映射——`dispatch_event` 全链路把 (port, ch) 落到
    /// `port*16+ch` 全局通道；超上限端口折叠到 15 块（B1 决策）。
    #[test]
    fn dispatch_event_maps_multi_port_to_global_channels() {
        let config = AudioRenderConfig {
            midi_max_port: 1, // 32 全局通道
            ..Default::default()
        };
        let mut recorder = RecordingSink::new(config.sample_rate);
        let mut conv = TickToTime::new(vec![(0, 120.0)], 480);
        let mut sink = VecSampleSink::new();
        {
            let mut processor =
                MidiEventProcessor::new(&config, &mut recorder, &mut conv, &mut sink);
            processor
                .dispatch_event(&note_on(0, 60), 0)
                .expect("端口 0 note on");
            processor
                .dispatch_event(&note_on(5, 61), 1)
                .expect("端口 1 note on");
            processor
                .dispatch_event(&note_on(9, 36), 7)
                .expect("上限内端口按自身块映射");
            processor
                .dispatch_event(&note_on(3, 62), 20)
                .expect("超上限端口折叠");
        }
        assert_eq!(
            recorder.event_channels(),
            vec![0, 16 + 5, 7 * 16 + 9, 15 * 16 + 3],
            "(port,ch) 必须映射为 port*16+ch；port>=16 折叠到 15 块"
        );
    }

    /// #102 ①：单端口恒等映射（端口被忽略，零行为变化基线）。
    #[test]
    fn dispatch_event_single_port_is_identity() {
        let config = AudioRenderConfig::default(); // midi_max_port == 0
        let mut recorder = RecordingSink::new(config.sample_rate);
        let mut conv = TickToTime::new(vec![(0, 120.0)], 480);
        let mut sink = VecSampleSink::new();
        {
            let mut processor =
                MidiEventProcessor::new(&config, &mut recorder, &mut conv, &mut sink);
            processor
                .dispatch_event(&note_on(5, 60), 7)
                .expect("单端口忽略端口");
        }
        assert_eq!(recorder.event_channels(), vec![5], "单端口映射必须恒等");
    }

    /// #102 ①：Bank Select 模态切换先于触发 CC（方案 B 顺序契约，经全链路）。
    #[test]
    fn dispatch_event_percussion_switch_precedes_trigger_cc() {
        let config = AudioRenderConfig {
            midi_max_port: 1,
            ..Default::default()
        };
        let mut recorder = RecordingSink::new(config.sample_rate);
        let mut conv = TickToTime::new(vec![(0, 120.0)], 480);
        let mut sink = VecSampleSink::new();
        {
            let mut processor =
                MidiEventProcessor::new(&config, &mut recorder, &mut conv, &mut sink);
            let cc = TrackEventKind::Midi {
                channel: u4::from(2),
                message: MidiMessage::Controller {
                    controller: u7::from(0),
                    value: u7::from(120), // GS Rhythm → 鼓
                },
            };
            processor
                .dispatch_event(&cc, 1)
                .expect("端口 1 ch2 CC0=120");
        }
        let global = 16 + 2;
        assert_eq!(recorder.events.len(), 2, "模态切换 + 转发 CC 恰两条");
        assert!(
            matches!(
                recorder.events.first(),
                Some(SynthEvent::Channel(ch, ChannelEvent::Config(
                    ChannelConfigEvent::SetPercussionMode(true)
                ))) if *ch == global
            ),
            "必须先对全局通道 {global} 下发 SetPercussionMode(true)"
        );
        assert!(
            matches!(
                recorder.events.get(1),
                Some(SynthEvent::Channel(ch, ChannelEvent::Audio(
                    ChannelAudioEvent::Control(ControlEvent::Raw(0, 120))
                ))) if *ch == global
            ),
            "随后才转发原始 CC"
        );
    }

    // ── REND-002 尾部收尾：NaN 旁路回归 ─────────────────────────────

    /// 尾部批次必须净化非有限样本并计数。
    ///
    /// 用**限幅器关闭**构造：限幅器开启时它自己也会净化输入，会掩盖尾部缺失
    /// 净化的缺陷——那正是本回归要锁的场景（NaN 直写 WAV）。
    #[test]
    fn tail_batch_purifies_non_finite_without_limiter() {
        use crate::audio::stream::VecSampleSink;
        use crate::audio::tick_conv::TickToTime;
        use xsynth_core::channel_group::ChannelGroup;

        let config = AudioRenderConfig {
            apply_limiter: false,
            ..Default::default()
        };
        let mut group = ChannelGroup::new(config.build_group_config());
        let mut conv = TickToTime::new(vec![(0, 120.0)], 480);
        let mut sink = VecSampleSink::new();
        {
            let mut processor = MidiEventProcessor::new(&config, &mut group, &mut conv, &mut sink);
            let mut buffer = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.25];
            let silent = processor
                .write_tail_batch(&mut buffer)
                .expect("尾部批次应成功");

            assert!(
                buffer.iter().all(|s| s.is_finite()),
                "尾部批次必须净化非有限样本: {buffer:?}"
            );
            assert_eq!(buffer[..3], [0.0, 0.0, 0.0], "非有限样本应归零");
            assert!(
                (buffer[3] - 0.25).abs() < f32::EPSILON,
                "有限样本不得被改动"
            );
            assert_eq!(processor.non_finite_total(), 3, "计数必须覆盖尾部净化");
            assert!(!silent, "仍含 0.25 有效样本，不得判静音");
        }

        let samples = sink.into_samples();
        assert!(
            samples.iter().all(|s| s.is_finite()),
            "写入 sink 的样本必须全部有限（限幅器关闭时旧实现直写 NaN）"
        );
    }

    /// 全非有限批次净化后必须判静音。
    ///
    /// 回归：`is_silent` 判据 `abs() < eps` 对 NaN 恒为 false，污染尾部会让
    /// 收尾循环跑满 120 批（每批 1 秒）→ 120 秒垃圾。
    #[test]
    fn tail_batch_all_non_finite_becomes_silent() {
        use crate::audio::stream::VecSampleSink;
        use crate::audio::tick_conv::TickToTime;
        use xsynth_core::channel_group::ChannelGroup;

        let config = AudioRenderConfig {
            apply_limiter: false,
            ..Default::default()
        };
        let mut group = ChannelGroup::new(config.build_group_config());
        let mut conv = TickToTime::new(vec![(0, 120.0)], 480);
        let mut sink = VecSampleSink::new();
        {
            let mut processor = MidiEventProcessor::new(&config, &mut group, &mut conv, &mut sink);
            let mut buffer = vec![f32::NAN; 8];
            let silent = processor
                .write_tail_batch(&mut buffer)
                .expect("尾部批次应成功");

            assert!(silent, "全非有限样本净化后应为静音（收尾必须能提前退出）");
            assert_eq!(processor.non_finite_total(), 8);
            assert!(buffer.iter().all(|&s| s == 0.0), "净化后应全为零");
        }
        let _ = sink.into_samples();
    }

    /// `finalize` 收尾有界、输出全有限（限幅器开/关双例）。
    ///
    /// 静音合成器首批即判静音：写出量应恰好 1 批。旧实现下若尾部受污染，
    /// 会一路写到批次上限（120 批）。
    #[test]
    fn finalize_tail_is_bounded_and_finite() {
        use crate::audio::stream::VecSampleSink;
        use crate::audio::tick_conv::TickToTime;
        use xsynth_core::channel_group::ChannelGroup;

        for apply_limiter in [true, false] {
            let config = AudioRenderConfig {
                apply_limiter,
                ..Default::default()
            };
            let batch = config.sample_rate as usize * usize::from(config.channels.channel_count());
            let mut group = ChannelGroup::new(config.build_group_config());
            let mut conv = TickToTime::new(vec![(0, 120.0)], 480);
            let mut sink = VecSampleSink::new();
            {
                let mut processor =
                    MidiEventProcessor::new(&config, &mut group, &mut conv, &mut sink);
                processor.finalize().expect("收尾应成功");
            }

            let samples = sink.into_samples();
            assert_eq!(
                samples.len(),
                batch,
                "静音尾部应只写 1 批（apply_limiter={apply_limiter}）"
            );
            assert!(
                samples.iter().all(|s| s.is_finite()),
                "收尾输出必须全有限（apply_limiter={apply_limiter}）"
            );
        }
    }
}
