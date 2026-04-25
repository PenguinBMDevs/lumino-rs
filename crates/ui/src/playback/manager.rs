//! 播放管理器
//!
//! 负责协调播放引擎和MIDI输出

use super::engine::{MidiMessage, NoteEvent, PlaybackEngine};
use super::{Playback, PlaybackAccessor, PlaybackState, TempoChange};
use lumino_cache::MidiCache;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

enum Command {
    SetMidiOutput(Box<dyn lumino_midi::OutputConnection>),
    ClearMidiOutput,
    SetNotes(Vec<NoteEvent>),
    SetTempoChanges(Vec<TempoChange>),
    SetCache(Option<Arc<MidiCache>>),
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
                        Command::SetTempoChanges(changes) => {
                            if let Ok(mut p) = engine.playback().lock() {
                                p.set_tempo_changes(changes);
                            }
                        }
                        Command::Play => engine.play(),
                        Command::Pause => engine.pause(),
                        Command::Stop => {
                            engine.stop();
                            if let Some(out) = &mut midi_output {
                                for channel in 0..16 {
                                    for key in 0..128 {
                                        let _ = out.note_off(channel, key, 0);
                                    }
                                }
                            }
                        }
                        Command::Seek(tick) => {
                            if let Some(out) = &mut midi_output {
                                for channel in 0..16 {
                                    for key in 0..128 {
                                        let _ = out.note_off(channel, key, 0);
                                    }
                                }
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
                    for msg in messages {
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
                        }
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
