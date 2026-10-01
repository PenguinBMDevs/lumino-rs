//! `MidiEventProcessor` 实现 — 事件分发、渲染驱动与限幅兜底
//!
//! 自 `event.rs` 拆分而来（零逻辑变更）。

use midly::{MidiMessage, PitchBend, TrackEventKind};
use tracing::info;
use xsynth_core::{
    AudioPipe,
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
    channel_group::{ChannelGroup, SynthEvent, SynthFormat},
};

use lumino_midi_model::multi_port::{PercussionTracker, track_global_channel};

use crate::audio::{
    config::AudioRenderConfig, limiter::AudioLimiter, stream::SampleSink, tick_conv::TickToTime,
};
use crate::error::ExportResult;

use super::MidiEventProcessor;

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

impl<'a> MidiEventProcessor<'a> {
    /// 创建 MIDI 事件处理器。
    ///
    /// 将 MIDI 事件按时间轴播放到给定的合成通道组与采样输出。
    pub fn new(
        config: &'a AudioRenderConfig,
        channel_group: &'a mut ChannelGroup,
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

            if let Some(limiter) = self.limiter.as_mut() {
                limiter.process(&mut buffer);
            }

            // 检测是否静音
            let is_silent = buffer.iter().all(|&s| s.abs() < 0.0001);

            self.sink.write_samples(&buffer)?;

            if is_silent {
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
}
