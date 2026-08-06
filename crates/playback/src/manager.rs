//! 播放管理器
//!
//! 负责协调播放引擎和MIDI输出

use crate::engine::{MidiMessage, MidiTrackEvent, PlaybackEngine};
use crate::{Playback, PlaybackAccessor, PlaybackState, TempoChange};
use crossbeam_channel::{Receiver, Sender, bounded};
use parking_lot::Mutex;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

/// 播放回调帧：播放线程每帧通过无锁 channel 推送给 UI 的实时播放快照。
///
/// 设计要点（无阻塞音频实时播放回调）：
/// - 由播放线程在 `update()` 后构造，通过 `try_send` 非阻塞投递，**绝不阻塞播放线程**。
/// - UI 线程每帧 `try_recv()` 非阻塞拉取最新帧，**彻底不再 `lock(playback)`**，
///   消除 UI 帧渲染与播放线程对 `Playback` 锁的争用（原 `current_tick()` 每帧抢锁导致卡顿）。
/// - channel 容量有限（环形缓冲），满则丢弃最旧帧，保证 UI 始终拿到最新进度。
#[derive(Debug, Clone, Copy)]
pub struct PlaybackFrame {
    /// 当前播放位置（tick）
    pub tick: f32,
    /// 当前播放状态
    pub state: PlaybackState,
    /// 当前 BPM（随 tempo 变化实时更新）
    pub bpm: f64,
}

/// 播放回调类型：在播放线程中调用，参数为实时播放帧快照。
///
/// 回调体在播放线程执行，必须**轻量且非阻塞**（仅做数据拷贝/发 channel），
/// 严禁执行 UI 渲染、文件 I/O 或任何可能长时间运行的操作。
pub type PlaybackCallback = Box<dyn FnMut(PlaybackFrame) + Send>;

enum Command {
    SetMidiOutput(Box<dyn lumino_midi_io::OutputConnection>),
    ClearMidiOutput,
    RebuildCurrentTrackQueue,
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
    /// 播放回调帧接收端（UI 线程持有，非阻塞 try_recv）
    frame_rx: Receiver<PlaybackFrame>,
    /// 最新播放帧缓存（播放线程每帧写，UI 非阻塞读，零消费冲突）
    last_frame: Arc<Mutex<Option<PlaybackFrame>>>,
    /// 用户注册的播放回调（在播放线程中调用，必须轻量非阻塞）
    callback: Arc<Mutex<Option<PlaybackCallback>>>,
    /// 线程句柄
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl PlaybackManager {
    /// 创建新的播放管理器
    pub fn new(division: u16) -> Self {
        let playback = Arc::new(Mutex::new(Playback::new(division)));
        let engine = PlaybackEngine::new(Arc::clone(&playback));

        let (sender, receiver) = mpsc::channel::<Command>();

        // 播放回调帧 channel：容量 8 的环形缓冲，满则丢最旧帧。
        // 播放线程 try_send 非阻塞投递，UI 线程 try_recv 非阻塞拉取。
        let (frame_tx, frame_rx) = bounded::<PlaybackFrame>(8);
        let last_frame = Arc::new(Mutex::new(None::<PlaybackFrame>));
        let callback = Arc::new(Mutex::new(None::<PlaybackCallback>));

        let thread_handle = thread::spawn({
            let frame_tx = frame_tx;
            let last_frame = Arc::clone(&last_frame);
            let callback = Arc::clone(&callback);
            move || {
                let mut engine = engine;
                let mut midi_output: Option<Box<dyn lumino_midi_io::OutputConnection>> = None;

                loop {
                    // 处理所有挂起的命令
                    while let Ok(cmd) = receiver.try_recv() {
                        if matches!(cmd, Command::Quit) {
                            return;
                        }
                        Self::handle_command(
                            cmd,
                            &mut engine,
                            &mut midi_output,
                            &frame_tx,
                            &last_frame,
                        );
                    }

                    // 仅在播放/暂停（仍需要定时推进）时启用高精度 1ms 定时循环；
                    // 空闲时阻塞等待命令，避免空转烧满一个核。
                    if engine.is_playing() {
                        // 更新引擎并发送 MIDI 消息
                        let messages = engine.update();
                        Self::flush_midi_messages(messages, &mut midi_output);

                        // 无阻塞播放回调：构造帧快照并 try_send 到 UI channel，
                        // 同时触发用户注册的回调（轻量非阻塞）。
                        // 满则丢最旧帧，保证 UI 始终拿到最新进度，绝不阻塞播放线程。
                        let bpm = engine.lock_playback().map_or(120.0, |p| p.current_bpm());
                        let frame = PlaybackFrame {
                            tick: engine.current_tick(),
                            state: engine.state(),
                            bpm,
                        };
                        let _ = frame_tx.try_send(frame);
                        *last_frame.lock() = Some(frame);
                        if let Some(cb) = callback.lock().as_mut() {
                            cb(frame);
                        }

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
                                Self::handle_command(
                                    cmd,
                                    &mut engine,
                                    &mut midi_output,
                                    &frame_tx,
                                    &last_frame,
                                );
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                // 空闲心跳：清空可能残留的 MIDI 状态（如暂停后的 all_notes_off 已在命令中处理）
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                }
            }
        });

        Self {
            sender,
            playback,
            frame_rx,
            last_frame,
            callback,
            thread_handle: Some(thread_handle),
        }
    }

    /// 处理单个播放控制命令。
    ///
    /// 从 `new()` 的线程闭包中提取，按命令更新引擎状态和 MIDI 输出连接。
    /// `frame_tx` / `last_frame` 用于在状态切换（Play/Pause/Stop）后主动推送一帧，
    /// 保证 UI 能立即感知状态变化（无需等待下一播放循环迭代）。
    fn handle_command(
        cmd: Command,
        engine: &mut PlaybackEngine,
        midi_output: &mut Option<Box<dyn lumino_midi_io::OutputConnection>>,
        frame_tx: &Sender<PlaybackFrame>,
        last_frame: &Arc<Mutex<Option<PlaybackFrame>>>,
    ) {
        match cmd {
            Command::SetMidiOutput(output) => *midi_output = Some(output),
            Command::ClearMidiOutput => *midi_output = None,
            Command::RebuildCurrentTrackQueue => engine.rebuild_current_track_queue(),
            Command::SetDocument(doc, track) => engine.set_document(doc, track),
            Command::SetMidiEvents(events) => engine.set_midi_events(events),
            Command::SetTempoChanges(changes) => {
                let mut playback_guard = engine.playback().lock();
                playback_guard.set_tempo_changes(changes);
            }
            Command::SetVelocityFilterThreshold(threshold) => {
                engine.set_velocity_filter_threshold(threshold);
            }
            Command::Play => {
                engine.play();
                Self::push_state_frame(engine, frame_tx, last_frame);
            }
            Command::Pause => {
                engine.pause();
                if let Some(out) = midi_output {
                    for ch in 0..16 {
                        let _ = out.control_change(ch, 64, 0);
                    }
                    let _ = out.all_notes_off();
                }
                Self::push_state_frame(engine, frame_tx, last_frame);
            }
            Command::Stop => {
                engine.stop();
                if let Some(out) = midi_output {
                    let _ = out.all_notes_off();
                    let _ = out.reset_control();
                }
                Self::push_state_frame(engine, frame_tx, last_frame);
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

    /// 主动推送一帧状态快照（用于 Play/Pause/Stop 等状态切换后）。
    ///
    /// 状态切换后播放线程可能进入空闲分支（不再每 1ms 推帧），
    /// 主动 try_send 一帧保证 UI 立即感知状态变化，绝不阻塞。
    fn push_state_frame(
        engine: &PlaybackEngine,
        frame_tx: &Sender<PlaybackFrame>,
        last_frame: &Arc<Mutex<Option<PlaybackFrame>>>,
    ) {
        let bpm = engine.lock_playback().map_or(120.0, |p| p.current_bpm());
        let frame = PlaybackFrame {
            tick: engine.current_tick(),
            state: engine.state(),
            bpm,
        };
        let _ = frame_tx.try_send(frame);
        *last_frame.lock() = Some(frame);
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

    /// 从当前 MIDI 文档重建当前音轨播放队列（当前轨与其他轨一致从 document 流式读取，
    /// 不再经 Vec<NoteEvent> 中转，避免每次编辑后全量克隆当前轨音符的 CPU 阻塞）
    pub fn rebuild_current_track_queue(&mut self) {
        let _ = self.sender.send(Command::RebuildCurrentTrackQueue);
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
            .map_or(PlaybackState::Stopped, |playback| playback.state())
    }

    /// 获取当前tick
    pub fn current_tick(&self) -> f32 {
        self.lock_playback()
            .map_or(0.0, |playback| playback.current_tick())
    }

    /// 获取当前BPM
    pub fn current_bpm(&self) -> f64 {
        self.lock_playback()
            .map_or(120.0, |playback| playback.current_bpm())
    }

    /// 注册播放回调（无阻塞音频实时播放回调）
    ///
    /// 回调在播放线程中调用，参数为实时播放帧快照（`PlaybackFrame`）。
    /// 回调体必须**轻量且非阻塞**（仅做数据拷贝/发 channel），
    /// 严禁执行 UI 渲染、文件 I/O 或任何可能长时间运行的操作。
    ///
    /// 重复调用会替换之前的回调。传入 `None` 清除回调。
    pub fn set_playback_callback(&mut self, callback: Option<PlaybackCallback>) {
        *self.callback.lock() = callback;
    }

    /// 非阻塞拉取最新播放帧（UI 线程调用）
    ///
    /// 通过无锁 channel `try_recv` 获取播放线程推送的最新 `PlaybackFrame`，
    /// **绝不阻塞 UI 线程**。channel 为环形缓冲，返回的是最近一次推送的帧
    /// （丢弃中间帧），保证 UI 始终拿到最新进度。
    ///
    /// 返回 `None` 表示当前无新帧（播放线程未运行或尚未产生帧）。
    pub fn try_recv_frame(&self) -> Option<PlaybackFrame> {
        self.frame_rx.try_recv().ok()
    }

    /// 非阻塞读取最新播放帧快照（UI 线程调用，不消费）
    ///
    /// 从 `last_frame` 缓存读取，与 `try_recv_frame` 互不干扰（后者消费 channel，
    /// 前者只读缓存）。用于 `is_playing()` 等需要反复查询状态、不希望吞掉帧的场景。
    /// **绝不阻塞 UI 线程**，零锁争用（parking_lot 读锁极轻）。
    pub fn last_frame(&self) -> Option<PlaybackFrame> {
        *self.last_frame.lock()
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
