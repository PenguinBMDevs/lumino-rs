//! 播放管理器
//!
//! 负责协调播放引擎和MIDI输出

use crate::engine::{MidiMessage, MidiTrackEvent, NoteEvent, PlaybackEngine};
use crate::{Playback, PlaybackAccessor, PlaybackState, TempoChange};
use parking_lot::Mutex;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

enum Command {
    SetMidiOutput(Box<dyn lumino_midi_io::OutputConnection>),
    ClearMidiOutput,
    SetCurrentTrackNotes(Vec<NoteEvent>),
    SetDocument(Arc<lumino_midi_loader::MidiDocument>, u16),
    SetMidiEvents(Vec<MidiTrackEvent>),
    SetTempoChanges(Vec<TempoChange>),
    SetVelocityFilterThreshold(u8),
    // 旧 SetCache/SetSkipTracksInCache 已移除（disk_cache future support）
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
            let mut midi_output: Option<Box<dyn lumino_midi_io::OutputConnection>> = None;

            loop {
                // 处理所有挂起的命令
                while let Ok(cmd) = receiver.try_recv() {
                    if matches!(cmd, Command::Quit) {
                        return;
                    }
                    Self::handle_command(cmd, &mut engine, &mut midi_output);
                }

                // 仅在播放/暂停（仍需要定时推进）时启用高精度 1ms 定时循环；
                // 空闲时阻塞等待命令，避免空转烧满一个核。
                if engine.is_playing() {
                    // 更新引擎并发送 MIDI 消息
                    let messages = engine.update();
                    Self::flush_midi_messages(messages, &mut midi_output);

                    // 高精度定时等待：sleep 大部分时间，最后自旋等待精确唤醒。
                    // Windows 默认定时器分辨率为 15.6ms，纯 sleep(1ms) 实际睡 15.6ms，
                    // 导致事件突发（15ms 的音符被一次性发送）。
                    // 混合策略：sleep(700μs) + spin(300μs) 实现接近 1ms 的精度。
                    let target = std::time::Instant::now() + Duration::from_millis(1);
                    thread::sleep(Duration::from_micros(700));
                    while std::time::Instant::now() < target {
                        std::hint::spin_loop();
                    }
                } else {
                    // 空闲分支：阻塞等待命令（50ms 超时兜底，处理 Seek/Pause 后残留引擎状态）
                    match receiver.recv_timeout(Duration::from_millis(50)) {
                        Ok(cmd) => {
                            if matches!(cmd, Command::Quit) {
                                return;
                            }
                            Self::handle_command(cmd, &mut engine, &mut midi_output);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // 空闲心跳：清空可能残留的 MIDI 状态（如暂停后的 all_notes_off 已在命令中处理）
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
        });

        Self {
            sender,
            playback,
            thread_handle: Some(thread_handle),
        }
    }

    /// 处理单个播放控制命令。
    ///
    /// 从 `new()` 的线程闭包中提取，按命令更新引擎状态和 MIDI 输出连接。
    fn handle_command(
        cmd: Command,
        engine: &mut PlaybackEngine,
        midi_output: &mut Option<Box<dyn lumino_midi_io::OutputConnection>>,
    ) {
        match cmd {
            Command::SetMidiOutput(output) => *midi_output = Some(output),
            Command::ClearMidiOutput => *midi_output = None,
            Command::SetCurrentTrackNotes(notes) => engine.set_current_track_notes(notes),
            Command::SetDocument(doc, track) => engine.set_document(doc, track),
            Command::SetMidiEvents(events) => engine.set_midi_events(events),
            Command::SetTempoChanges(changes) => {
                let mut p = engine.playback().lock();
                p.set_tempo_changes(changes);
            }
            Command::SetVelocityFilterThreshold(threshold) => {
                engine.set_velocity_filter_threshold(threshold);
            }
            Command::Play => engine.play(),
            Command::Pause => {
                engine.pause();
                if let Some(out) = midi_output {
                    for ch in 0..16 {
                        let _ = out.control_change(ch, 64, 0);
                    }
                    let _ = out.all_notes_off();
                }
            }
            Command::Stop => {
                engine.stop();
                if let Some(out) = midi_output {
                    let _ = out.all_notes_off();
                    let _ = out.reset_control();
                }
            }
            Command::Seek(tick) => {
                if let Some(out) = midi_output {
                    let _ = out.all_notes_off();
                    let _ = out.reset_control();
                }
                engine.seek(tick);
            }
            Command::SetLooping(looping) => engine.set_looping(looping),
            Command::SetLoopRange(start, end) => engine.set_loop_range(start, end),
            Command::ClearLoopRange => engine.clear_loop_range(),
            Command::Quit => {}
        }
    }

    /// 将引擎输出的 MIDI 消息发送到 MIDI 输出设备。
    fn flush_midi_messages(
        messages: &[MidiMessage],
        midi_output: &mut Option<Box<dyn lumino_midi_io::OutputConnection>>,
    ) {
        let Some(out) = midi_output else { return };
        let msg_count = messages.len();

        for msg in messages {
            match msg {
                MidiMessage::NoteOn {
                    channel,
                    key,
                    velocity,
                } => {
                    let _ = out.note_on(*channel, *key, *velocity);
                }
                MidiMessage::NoteOff { channel, key } => {
                    let _ = out.note_off(*channel, *key, 0);
                }
                MidiMessage::ControlChange {
                    channel,
                    controller,
                    value,
                } => {
                    tracing::debug!(
                        "PlaybackManager: 发送 CC ch={} cc={} val={}",
                        channel,
                        controller,
                        value,
                    );
                    let _ = out.control_change(*channel, *controller, *value);
                }
                MidiMessage::ProgramChange { channel, program } => {
                    let _ = out.program_change(*channel, *program);
                }
                MidiMessage::PitchBend { channel, value } => {
                    let _ = out.pitch_bend(*channel, *value);
                }
                MidiMessage::ChannelPressure { channel, pressure } => {
                    let _ = out.channel_pressure(*channel, *pressure);
                }
                MidiMessage::PolyPressure {
                    channel,
                    key,
                    pressure,
                } => {
                    let _ = out.poly_pressure(*channel, *key, *pressure);
                }
            }
        }
        if msg_count > 0 {
            tracing::trace!("PlaybackManager: sent {} MIDI events", msg_count);
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
    pub fn set_midi_output(&mut self, output: Box<dyn lumino_midi_io::OutputConnection>) {
        let _ = self.sender.send(Command::SetMidiOutput(output));
    }

    /// 移除MIDI输出
    pub fn clear_midi_output(&mut self) {
        let _ = self.sender.send(Command::ClearMidiOutput);
    }

    /// 设置当前音轨音符列表（用于编辑后的当前轨更新）
    pub fn set_current_track_notes(&mut self, notes: Vec<NoteEvent>) {
        let _ = self.sender.send(Command::SetCurrentTrackNotes(notes));
    }

    /// 设置 MIDI 文档引用（其他音轨从此流式读取）
    pub fn set_document(&mut self, doc: Arc<lumino_midi_loader::MidiDocument>, current_track: u16) {
        let _ = self.sender.send(Command::SetDocument(doc, current_track));
    }

    // 旧 set_cache/set_skip_tracks_in_cache 已移除（disk_cache future support）

    /// 设置非音符MIDI事件列表
    pub fn set_midi_events(&mut self, events: Vec<MidiTrackEvent>) {
        let _ = self.sender.send(Command::SetMidiEvents(events));
    }

    /// 设置速度变化
    pub fn set_tempo_changes(&mut self, changes: Vec<TempoChange>) {
        let _ = self.sender.send(Command::SetTempoChanges(changes));
    }

    /// 设置力度过滤阈值（语义过滤，非性能节流）
    pub fn set_velocity_filter_threshold(&mut self, threshold: u8) {
        let _ = self
            .sender
            .send(Command::SetVelocityFilterThreshold(threshold));
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
