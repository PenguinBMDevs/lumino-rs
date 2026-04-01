//! 播放管理器
//!
//! 负责协调播放引擎和MIDI输出

use super::engine::{MidiMessage, NoteEvent, PlaybackEngine};
use super::{Playback, PlaybackState, TempoChange};
use std::sync::{Arc, Mutex};

/// 播放管理器
pub struct PlaybackManager {
    /// 播放引擎
    engine: PlaybackEngine,
    /// 播放器引用（共享）
    playback: Arc<Mutex<Playback>>,
    /// MIDI输出连接（可选）
    midi_output: Option<Box<dyn lumino_midi::OutputConnection>>,
}

impl PlaybackManager {
    /// 创建新的播放管理器
    pub fn new(division: u16) -> Self {
        let playback = Arc::new(Mutex::new(Playback::new(division)));
        let engine = PlaybackEngine::new(Arc::clone(&playback));

        Self {
            engine,
            playback,
            midi_output: None,
        }
    }

    /// 设置MIDI输出
    pub fn set_midi_output(&mut self, output: Box<dyn lumino_midi::OutputConnection>) {
        self.midi_output = Some(output);
    }

    /// 移除MIDI输出
    pub fn clear_midi_output(&mut self) {
        self.midi_output = None;
    }

    /// 设置音符列表
    pub fn set_notes(&mut self, notes: Vec<NoteEvent>) {
        self.engine.set_notes(notes);
    }

    /// 设置速度变化
    pub fn set_tempo_changes(&mut self, changes: Vec<TempoChange>) {
        let mut playback = self.playback.lock().unwrap();
        playback.set_tempo_changes(changes);
    }

    /// 更新速度变化（别名方法）
    pub fn update_tempo_changes(&mut self, changes: Vec<TempoChange>) {
        self.set_tempo_changes(changes);
    }

    /// 播放
    pub fn play(&mut self) {
        self.engine.play();
    }

    /// 暂停
    pub fn pause(&mut self) {
        self.engine.pause();
    }

    /// 停止
    pub fn stop(&mut self) {
        self.engine.stop();
        // 发送所有音符的NoteOff
        self.send_all_notes_off();
    }

    /// 跳转
    pub fn seek(&mut self, tick: f32) {
        self.send_all_notes_off();
        self.engine.seek(tick);
    }

    /// 获取播放状态
    pub fn state(&self) -> PlaybackState {
        self.engine.state()
    }

    /// 获取当前tick
    pub fn current_tick(&self) -> f32 {
        self.engine.current_tick()
    }

    /// 获取当前BPM
    pub fn current_bpm(&self) -> f64 {
        self.playback.lock().unwrap().current_bpm()
    }

    /// 更新播放（应在定时器中调用）
    pub fn update(&mut self) {
        let messages = self.engine.update();

        // 发送MIDI消息
        if let Some(output) = &mut self.midi_output {
            for msg in messages {
                let result = match msg {
                    MidiMessage::NoteOn {
                        channel,
                        key,
                        velocity,
                    } => output.note_on(channel, key, velocity),
                    MidiMessage::NoteOff { channel, key } => output.note_off(channel, key, 0),
                };

                if let Err(e) = result {
                    tracing::error!("MIDI输出错误: {:?}", e);
                }
            }
        }
    }

    /// 发送所有音符关闭
    fn send_all_notes_off(&mut self) {
        if let Some(output) = &mut self.midi_output {
            // 发送All Notes Off (CC 123) 到所有通道
            for channel in 0..16 {
                // 简化实现：直接关闭所有可能的音符
                for key in 0..128 {
                    let _ = output.note_off(channel, key, 0);
                }
            }
        }
    }

    /// 设置循环
    pub fn set_looping(&mut self, looping: bool) {
        self.engine.set_looping(looping);
    }

    /// 设置循环范围
    pub fn set_loop_range(&mut self, start: f32, end: f32) {
        self.engine.set_loop_range(start, end);
    }

    /// 清除循环范围
    pub fn clear_loop_range(&mut self) {
        self.engine.clear_loop_range();
    }
}
