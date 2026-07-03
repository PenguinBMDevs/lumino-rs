//! AudioEngine — 封装 xsynth-core::ChannelGroup，提供同步渲染接口。
//!
//! Renderer 线程在 `render()` 中调用 `channel_group.read_samples()`，
//! 所有事件派发和渲染在同一个调用栈里完成，避免跨线程调度抖动。

use std::sync::Arc;

use xsynth_core::channel::{
    ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ChannelInitOptions, ControlEvent,
};
use xsynth_core::channel_group::{
    ChannelGroup, ChannelGroupConfig, ParallelismOptions, SynthEvent, SynthFormat,
};
use xsynth_core::effects::VolumeLimiter;
use xsynth_core::soundfont::SoundfontBase;
use xsynth_core::{AudioStreamParams, ChannelCount};

use crate::audio_model::{ActiveNote, PreparedModel, tick_to_sample};
use crate::channel::ChannelState;
use crate::engine_render::RenderCursor;
use crate::engine_state::EngineState;

/// 播放状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

/// 渲染参数。
#[derive(Clone, Copy)]
pub struct RenderConfig {
    pub sample_rate: u32,
    pub block_size: usize,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            block_size: 256,
        }
    }
}

/// 音频引擎 — 封装 ChannelGroup + 渲染状态。
pub struct AudioEngine {
    pub(crate) channel_group: ChannelGroup,
    pub(crate) limiter: VolumeLimiter,
    pub(crate) state: EngineState,
    pub(crate) config: RenderConfig,
    pub(crate) cursor: RenderCursor,
    pub(crate) play_state: PlayState,
    pub(crate) active_notes: Vec<ActiveNote>,
    pub(crate) note_cursors: [usize; 128],
    pub(crate) channel_states: [ChannelState; 16],
    pub(crate) cc_cursor: usize,
}

impl AudioEngine {
    pub(crate) fn new(config: RenderConfig) -> Self {
        let audio_params = AudioStreamParams::new(config.sample_rate, ChannelCount::Stereo);
        let channel_init_options = ChannelInitOptions {
            fade_out_killing: true,
            max_voices_per_key: Some(8),
            global_voice_limit: Some(4096),
        };
        let group_config = ChannelGroupConfig {
            channel_init_options,
            format: SynthFormat::Midi,
            audio_params,
            parallelism: ParallelismOptions::default(),
        };
        let channel_group = ChannelGroup::new(group_config);
        let limiter = VolumeLimiter::new(ChannelCount::Stereo.count());

        Self {
            channel_group,
            limiter,
            state: EngineState::new(),
            config,
            cursor: RenderCursor::new(),
            play_state: PlayState::Stopped,
            active_notes: Vec::with_capacity(512),
            note_cursors: [0; 128],
            channel_states: std::array::from_fn(|_| ChannelState::default()),
            cc_cursor: 0,
        }
    }

    /// 加载预计算模型，重置渲染状态。
    pub(crate) fn load_model(&mut self, model: PreparedModel) {
        tracing::debug!(
            "[AUDIO-ENGINE] load_model: duration={}, notes_by_key={}, tempo_segments={}, cc_events={}",
            model.duration_samples,
            model.notes_by_key.is_some(),
            model.tempo_segments.len(),
            model.cc_events.len(),
        );
        self.reset_all();
        self.state.load_model(model);
        self.reset_cursors();
        self.play_state = PlayState::Stopped;
    }

    /// 设置音色库。
    pub(crate) fn set_soundfonts(&mut self, sfs: Vec<Arc<dyn SoundfontBase>>) {
        self.channel_group
            .send_event(SynthEvent::AllChannels(ChannelEvent::Config(
                ChannelConfigEvent::SetSoundfonts(sfs),
            )));
    }

    /// 开始播放。
    pub(crate) fn play(&mut self) {
        if self.play_state != PlayState::Playing {
            self.play_state = PlayState::Playing;
        }
    }

    /// 暂停播放。
    pub(crate) fn pause(&mut self) {
        self.play_state = PlayState::Paused;
        self.all_notes_off();
    }

    /// 停止播放并回到起点。
    pub(crate) fn stop(&mut self) {
        self.play_state = PlayState::Stopped;
        self.all_notes_off();
        self.seek_to_sample(0);
    }

    /// Seek 到指定 sample 位置（包含 Chase 机制）。
    pub(crate) fn seek_to_sample(&mut self, sample: u64) {
        self.all_notes_off();
        self.cursor.set_position(sample);
        self.reset_note_cursors_for_position(sample);
        self.chase_control_state(sample);
    }

    /// Seek 到指定 tick 位置。
    pub(crate) fn seek_to_tick(&mut self, tick: u32) {
        if let Some(model) = self.state.model() {
            let sample = tick_to_sample(
                tick as u64,
                &model.tempo_segments,
                self.config.sample_rate as f64,
            );
            self.seek_to_sample(sample);
        }
    }

    /// 发送即时 NoteOn（用于试听）。
    pub(crate) fn preview_note_on(&mut self, channel: u8, key: u8, velocity: u8) {
        if channel >= 16 {
            tracing::warn!("preview_note_on: channel {} 超出范围 (0-15)", channel);
            return;
        }
        if key >= 128 {
            tracing::warn!("preview_note_on: key {} 超出范围 (0-127)", key);
            return;
        }
        self.channel_group.send_event(SynthEvent::Channel(
            channel as u32,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOn { key, vel: velocity }),
        ));
    }

    /// 发送即时 NoteOff（用于试听）。
    pub(crate) fn preview_note_off(&mut self, channel: u8, key: u8) {
        if channel >= 16 {
            return;
        }
        if key >= 128 {
            return;
        }
        self.channel_group.send_event(SynthEvent::Channel(
            channel as u32,
            ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key }),
        ));
    }

    /// 发送即时 CC 事件（用于试听）。
    pub(crate) fn preview_cc(&mut self, channel: u8, controller: u8, value: u8) {
        if channel >= 16 {
            tracing::warn!("preview_cc: channel {} 超出范围 (0-15)", channel);
            return;
        }
        let event = ChannelAudioEvent::Control(ControlEvent::Raw(controller, value));
        self.channel_states[channel as usize].apply(&event);
        self.channel_group.send_event(SynthEvent::Channel(
            channel as u32,
            ChannelEvent::Audio(event),
        ));
    }

    /// 发送即时 ProgramChange（用于试听）。
    pub(crate) fn preview_program_change(&mut self, channel: u8, program: u8) {
        if channel >= 16 {
            tracing::warn!(
                "preview_program_change: channel {} 超出范围 (0-15)",
                channel
            );
            return;
        }
        let event = ChannelAudioEvent::ProgramChange(program);
        self.channel_states[channel as usize].apply(&event);
        self.channel_group.send_event(SynthEvent::Channel(
            channel as u32,
            ChannelEvent::Audio(event),
        ));
    }

    /// 发送即时 PitchBend（用于试听）。
    pub(crate) fn preview_pitch_bend(&mut self, channel: u8, value: f32) {
        if channel >= 16 {
            tracing::warn!("preview_pitch_bend: channel {} 超出范围 (0-15)", channel);
            return;
        }
        let event = ChannelAudioEvent::Control(ControlEvent::PitchBendValue(value));
        self.channel_states[channel as usize].apply(&event);
        self.channel_group.send_event(SynthEvent::Channel(
            channel as u32,
            ChannelEvent::Audio(event),
        ));
    }

    /// 全部音符关闭。
    pub(crate) fn all_notes_off(&mut self) {
        for ch in 0u32..16 {
            self.channel_group.send_event(SynthEvent::Channel(
                ch,
                ChannelEvent::Audio(ChannelAudioEvent::AllNotesOff),
            ));
        }
        self.active_notes.clear();
    }

    /// 重置所有控制器。
    pub(crate) fn reset_all(&mut self) {
        self.all_notes_off();
        for ch in 0u32..16 {
            self.channel_group.send_event(SynthEvent::Channel(
                ch,
                ChannelEvent::Audio(ChannelAudioEvent::AllNotesKilled),
            ));
            self.channel_group.send_event(SynthEvent::Channel(
                ch,
                ChannelEvent::Audio(ChannelAudioEvent::ResetControl),
            ));
        }
        self.channel_states = std::array::from_fn(|_| ChannelState::default());
    }

    /// 获取当前播放位置（sample）。
    pub(crate) fn position_samples(&self) -> u64 {
        self.cursor.position
    }

    /// 获取当前播放位置（tick）。
    pub(crate) fn position_tick(&self) -> f64 {
        if let Some(model) = self.state.model() {
            crate::audio_model::sample_to_tick(
                self.cursor.position,
                &model.tempo_segments,
                self.config.sample_rate as f64,
            )
        } else {
            0.0
        }
    }

    /// 获取总时长（sample）。
    pub(crate) fn duration_samples(&self) -> u64 {
        self.state.model().map(|m| m.duration_samples).unwrap_or(0)
    }

    fn reset_cursors(&mut self) {
        self.cursor = RenderCursor::new();
        self.note_cursors = [0; 128];
        self.cc_cursor = 0;
        self.active_notes.clear();
    }

    fn reset_note_cursors_for_position(&mut self, sample: u64) {
        let model = match self.state.model() {
            Some(m) => m,
            None => return,
        };
        let notes_by_key = match model.notes_by_key.as_ref() {
            Some(n) => n,
            None => return, // 实时播放模式，无模型事件需派发
        };
        for key in 0..128 {
            let bucket = &notes_by_key[key];
            self.note_cursors[key] = bucket.partition_point(|n| {
                let start_sample = tick_to_sample(
                    n.start_tick as u64,
                    &model.tempo_segments,
                    self.config.sample_rate as f64,
                );
                start_sample < sample
            });
        }
    }

    fn chase_control_state(&mut self, sample: u64) {
        if let Some(model) = self.state.model() {
            // 重置通道状态为默认
            self.channel_states = std::array::from_fn(|_| ChannelState::default());
            self.cc_cursor = 0;

            // 重放所有 sample 位置之前的控制事件
            for cc in &model.cc_events {
                if cc.sample > sample {
                    break;
                }
                self.channel_states[cc.channel as usize].apply(&cc.event);
                self.cc_cursor += 1;
            }

            // 将重建的状态发送到 ChannelGroup
            for (ch, state) in self.channel_states.iter().enumerate() {
                state.send_to(ch as u32, &mut self.channel_group);
            }
        }
    }
}
