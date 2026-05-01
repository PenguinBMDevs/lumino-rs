//! 播放管理器
//!
//! 负责协调播放引擎和MIDI输出

use super::engine::{MidiMessage, MidiTrackEvent, NoteEvent, PlaybackEngine};
use super::{Playback, PlaybackAccessor, PlaybackState, TempoChange};
use lumino_cache::MidiCache;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

enum Command {
    SetMidiOutput(Box<dyn lumino_midi::OutputConnection>),
    ClearMidiOutput,
    SetNotes(Vec<NoteEvent>),
    SetMidiEvents(Vec<MidiTrackEvent>),
    SetTempoChanges(Vec<TempoChange>),
    SetCache(Option<Arc<MidiCache>>),
    /// 设置缓存流式读取时需要跳过的音轨（这些音轨已通过 self.notes 覆盖）
    SetSkipTracksInCache(Vec<u16>),
    Play,
    Pause,
    Stop,
    Seek(f32),
    SetLooping(bool),
    SetLoopRange(f32, f32),
    ClearLoopRange,
    Quit,
}

/// 播放管理器
pub struct PlaybackManager {
    /// 命令发送者
    sender: mpsc::Sender<Command>,
    /// 播放器引用（共享）
    playback: Arc<Mutex<Playback>>,
    /// 线程句柄
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl PlaybackManager {
    /// 创建新的播放管理器
    pub fn new(division: u16) -> Self {
        let playback = Arc::new(Mutex::new(Playback::new(division)));
        let engine = PlaybackEngine::new(Arc::clone(&playback));

        let (sender, receiver) = mpsc::channel::<Command>();

        let thread_handle = thread::spawn(move || {
            let mut engine = engine;
            let mut midi_output: Option<Box<dyn lumino_midi::OutputConnection>> = None;

            loop {
                // 处理所有挂起的命令
                while let Ok(cmd) = receiver.try_recv() {
                    match cmd {
                        Command::SetMidiOutput(output) => midi_output = Some(output),
                        Command::ClearMidiOutput => midi_output = None,
                        Command::SetNotes(notes) => engine.set_notes(notes),
                        Command::SetCache(cache) => engine.set_cache(cache),
                        Command::SetSkipTracksInCache(tracks) => {
                            engine.set_skip_tracks_in_cache(tracks)
                        }
                        Command::SetMidiEvents(events) => engine.set_midi_events(events),
                        Command::SetTempoChanges(changes) => {
                            if let Ok(mut p) = engine.playback().lock() {
                                p.set_tempo_changes(changes);
                            }
                        }
                        Command::Play => engine.play(),
                        Command::Pause => {
                            engine.pause();
                            if let Some(out) = &mut midi_output {
                                // 释放所有通道的延音踏板，防止音符永久保持
                                for ch in 0..16 {
                                    let _ = out.control_change(ch, 64, 0);
                                }
                                // 停止当前发声的音符（保留 Release 阶段）
                                let _ = out.all_notes_off();
                            }
                        }
                        Command::Stop => {
                            engine.stop();
                            if let Some(out) = &mut midi_output {
                                let _ = out.all_notes_off();
                                let _ = out.reset_control();
                            }
                        }
                        Command::Seek(tick) => {
                            if let Some(out) = &mut midi_output {
                                let _ = out.all_notes_off();
                                let _ = out.reset_control();
                            }
                            engine.seek(tick);
                        }
                        Command::SetLooping(looping) => engine.set_looping(looping),
                        Command::SetLoopRange(start, end) => engine.set_loop_range(start, end),
                        Command::ClearLoopRange => engine.clear_loop_range(),
                        Command::Quit => return,
                    }
                }

                // 更新引擎并发送MIDI消息
                let messages = engine.update();
                if let Some(out) = &mut midi_output {
                    let msg_count = messages.len();
                    for (i, msg) in messages.into_iter().enumerate() {
                        // 每 20 条消息让出 CPU 给 xsynth 通道线程处理积压事件
                        // 防止 seek 后大量事件瞬间涌入导致 buffer underrun
                        if i > 0 && i % 20 == 0 {
                            std::thread::yield_now();
                        }
                        match msg {
                            MidiMessage::NoteOn {
                                channel,
                                key,
                                velocity,
                            } => {
                                let _ = out.note_on(channel, key, velocity);
                            }
                            MidiMessage::NoteOff { channel, key } => {
                                let _ = out.note_off(channel, key, 0);
                            }
                            MidiMessage::ControlChange {
                                channel,
                                controller,
                                value,
                            } => {
                                let _ = out.control_change(channel, controller, value);
                            }
                            MidiMessage::ProgramChange { channel, program } => {
                                let _ = out.program_change(channel, program);
                            }
                            MidiMessage::PitchBend { channel, value } => {
                                let _ = out.pitch_bend(channel, value);
                            }
                            MidiMessage::ChannelPressure { channel, pressure } => {
                                let _ = out.channel_pressure(channel, pressure);
                            }
                            MidiMessage::PolyPressure {
                                channel,
                                key,
                                pressure,
                            } => {
                                let _ = out.poly_pressure(channel, key, pressure);
                            }
                        }
                    }
                    if msg_count > 0 {
                        tracing::trace!("PlaybackManager: sent {} MIDI events", msg_count);
                    }
                }

                // 睡眠以避免空转，休眠1ms保证音频高精度调度
                thread::sleep(Duration::from_millis(1));
            }
        });

        Self {
            sender,
            playback,
            thread_handle: Some(thread_handle),
        }
    }
}

impl Drop for PlaybackManager {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Quit);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl PlaybackAccessor for PlaybackManager {
    fn playback(&self) -> &Arc<Mutex<Playback>> {
        &self.playback
    }
}

impl PlaybackManager {
    pub fn set_midi_output(&mut self, output: Box<dyn lumino_midi::OutputConnection>) {
        let _ = self.sender.send(Command::SetMidiOutput(output));
    }

    /// 移除MIDI输出
    pub fn clear_midi_output(&mut self) {
        let _ = self.sender.send(Command::ClearMidiOutput);
    }

    /// 设置音符列表
    pub fn set_notes(&mut self, notes: Vec<NoteEvent>) {
        let _ = self.sender.send(Command::SetNotes(notes));
    }

    /// 设置 MIDI 缓存
    pub fn set_cache(&mut self, cache: Option<Arc<MidiCache>>) {
        let _ = self.sender.send(Command::SetCache(cache));
    }

    /// 设置缓存流式读取时需要跳过的音轨
    /// 这些音轨已通过 set_notes 提供了完整事件，若不跳过会导致事件重复
    pub fn set_skip_tracks_in_cache(&mut self, tracks: Vec<u16>) {
        let _ = self.sender.send(Command::SetSkipTracksInCache(tracks));
    }

    /// 设置非音符MIDI事件列表
    pub fn set_midi_events(&mut self, events: Vec<MidiTrackEvent>) {
        let _ = self.sender.send(Command::SetMidiEvents(events));
    }

    /// 设置速度变化
    pub fn set_tempo_changes(&mut self, changes: Vec<TempoChange>) {
        let _ = self.sender.send(Command::SetTempoChanges(changes));
    }

    /// 更新速度变化（别名方法）
    pub fn update_tempo_changes(&mut self, changes: Vec<TempoChange>) {
        self.set_tempo_changes(changes);
    }

    /// 播放
    pub fn play(&mut self) {
        let _ = self.sender.send(Command::Play);
    }

    /// 暂停
    pub fn pause(&mut self) {
        let _ = self.sender.send(Command::Pause);
    }

    /// 停止
    pub fn stop(&mut self) {
        let _ = self.sender.send(Command::Stop);
    }

    /// 跳转
    pub fn seek(&mut self, tick: f32) {
        let _ = self.sender.send(Command::Seek(tick));
    }

    /// 获取播放状态
    pub fn state(&self) -> PlaybackState {
        self.lock_playback()
            .map_or(PlaybackState::Stopped, |p| p.state())
    }

    /// 获取当前tick
    pub fn current_tick(&self) -> f32 {
        self.lock_playback().map_or(0.0, |p| p.current_tick())
    }

    /// 获取当前BPM
    pub fn current_bpm(&self) -> f64 {
        self.lock_playback().map_or(120.0, |p| p.current_bpm())
    }

    /// 更新播放（由于已在独立线程更新，这里为空操作）
    pub fn update(&mut self) {
        // No-op
    }

    /// 设置循环
    pub fn set_looping(&mut self, looping: bool) {
        let _ = self.sender.send(Command::SetLooping(looping));
    }

    /// 设置循环范围
    pub fn set_loop_range(&mut self, start: f32, end: f32) {
        let _ = self.sender.send(Command::SetLoopRange(start, end));
    }

    /// 清除循环范围
    pub fn clear_loop_range(&mut self) {
        let _ = self.sender.send(Command::ClearLoopRange);
    }
}
