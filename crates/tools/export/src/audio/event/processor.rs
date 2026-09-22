//! `MidiEventProcessor` 实现 — 事件分发、渲染驱动与限幅兜底
//!
//! 自 `event.rs` 拆分而来（零逻辑变更）。

use midly::{MidiMessage, PitchBend, TrackEventKind};
use tracing::info;
use xsynth_core::{
    AudioPipe,
    channel::{ChannelAudioEvent, ChannelEvent, ControlEvent},
    channel_group::{ChannelGroup, SynthEvent},
};

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
        MidiEventProcessor {
            config,
            channel_group,
            tick_conv,
            sink,
            sample_rate: params.sample_rate,
            channel_count: params.channels.count(),
            vec_pool: Vec::new(),
            limiter,
        }
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
    /// 返回因 `note_force_end_delay` 额外渲染的帧数（无则 0）——调用方必须把它
    /// 计入采样时钟，避免后续重复渲染。
    pub(crate) fn dispatch_event(&mut self, event_kind: &TrackEventKind) -> ExportResult<u64> {
        if let Some(ctrl) = &self.config.control {
            ctrl.wait_if_paused();
            ctrl.check_abort()?;
        }
        let mut extra_frames = 0_u64;

        // 发送 MIDI 事件到合成器
        if let TrackEventKind::Midi { channel, message } = event_kind {
            let ch = channel.as_int() as u32;
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
