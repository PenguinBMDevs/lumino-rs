//! 公开 API — 启动 cpal 音频后端 + renderer 线程。
//!
//! cpal 回调只做 `ring.pop_into()` + 静音填充，**永不阻塞**。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use cpal::traits::{DeviceTrait, HostTrait};
use crossbeam_channel::{unbounded, Receiver, Sender};
use lumino_midi_loader::MidiDocument;

use crate::audio_renderer::{self, EngineStateSnapshot};
use crate::audio_ring::{AudioRing, AudioRingConsumer};
use crate::engine::{AudioEngine, PlayState, RenderConfig};
use crate::prepare_model::{self, WorkerResult};

/// 发送给音频引擎的命令。
#[derive(Debug, Clone)]
pub enum AudioCommand {
    Play,
    Pause,
    Stop,
    SeekSample(u64),
    SeekTick(u32),
    NoteOn { channel: u8, key: u8, velocity: u8 },
    NoteOff { channel: u8, key: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    PitchBend { channel: u8, value: f32 },
    AllNotesOff,
    ResetAll,
    Shutdown,
}

/// 音频引擎句柄 — 持有所有线程和 cpal 流。
pub struct CpalAudioHandle {
    /// 发送命令到 renderer 线程。
    pub cmd_tx: Sender<AudioCommand>,
    /// 接收引擎状态快照（播放位置等）。
    pub state_rx: Receiver<EngineStateSnapshot>,
    worker_tx: Sender<WorkerMessage>,
    stream: cpal::Stream,
    renderer_handle: Option<JoinHandle<()>>,
    worker_handle: Option<JoinHandle<()>>,
    shutdown_flag: Arc<AtomicBool>,
    pub engine: Arc<Mutex<AudioEngine>>,
}

/// Worker 线程消息。
enum WorkerMessage {
    LoadModel {
        doc: Arc<MidiDocument>,
        soundfont_paths: Vec<PathBuf>,
    },
    Shutdown,
}

/// 估算模型需要的通道数（用于 cpal 配置）。
pub fn channels_for_model(_doc: &MidiDocument) -> u16 {
    2 // stereo
}

/// 启动 cpal 音频后端 + renderer 线程。
///
/// 返回 `(AudioHandle, engine)`，调用方持有 engine 用于直接查询状态。
pub fn spawn_cpal_audio(
    sample_rate: u32,
) -> Result<(CpalAudioHandle, Arc<Mutex<AudioEngine>>), AudioSpawnError> {
    // 1. 初始化 cpal
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(AudioSpawnError::NoOutputDevice)?;

    let _supported_config = device
        .supported_output_configs()
        .map_err(|e| AudioSpawnError::DeviceError(e.to_string()))?
        .find(|c| {
            c.channels() == 2
                && c.min_sample_rate().0 <= sample_rate
                && c.max_sample_rate().0 >= sample_rate
        })
        .or_else(|| device.supported_output_configs().ok()?.next())
        .ok_or(AudioSpawnError::NoSupportedConfig)?;

    let stream_config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let actual_sr = sample_rate;

    // 2. 创建 AudioEngine
    let config = RenderConfig {
        sample_rate: actual_sr,
        block_size: 256,
    };
    let engine = Arc::new(Mutex::new(AudioEngine::new(config)));

    // 3. 创建 ring buffer（约 0.5 秒缓冲）
    let ring_capacity = (actual_sr as usize * 2).next_power_of_two();
    let ring = AudioRing::new(ring_capacity);
    let (producer, consumer) = ring.split();

    // 4. 创建 cpal 输出流 — 回调永不阻塞
    let stream = build_cpal_stream(&device, &stream_config, consumer)?;

    // 5. 启动 cpal 音频流（必须显式调用 play()）
    stream.play()?;

    // 6. 创建 channels
    let (cmd_tx, cmd_rx) = unbounded::<AudioCommand>();
    let (state_tx, state_rx) = unbounded::<EngineStateSnapshot>();
    let (worker_tx, worker_rx) = unbounded::<WorkerMessage>();
    let shutdown_flag = Arc::new(AtomicBool::new(false));

    // 7. 启动 renderer 线程
    let engine_for_renderer = Arc::clone(&engine);
    let shutdown_flag_renderer = Arc::clone(&shutdown_flag);
    let renderer_handle = thread::Builder::new()
        .name("lumino-audio-renderer".to_string())
        .spawn(move || {
            audio_renderer::run_audio_renderer(
                engine_for_renderer,
                producer,
                cmd_rx,
                state_tx,
                shutdown_flag_renderer,
            );
        })?;

    // 8. 启动 worker 线程
    let engine_for_worker = Arc::clone(&engine);
    let worker_handle = thread::Builder::new()
        .name("lumino-audio-worker".to_string())
        .spawn(move || {
            run_worker_thread(engine_for_worker, worker_rx, actual_sr);
        })?;

    let handle = CpalAudioHandle {
        cmd_tx,
        state_rx,
        worker_tx: worker_tx.clone(),
        stream,
        renderer_handle: Some(renderer_handle),
        worker_handle: Some(worker_handle),
        shutdown_flag,
        engine: Arc::clone(&engine),
    };

    Ok((handle, engine))
}

/// 构建 cpal 输出流 — 回调永不阻塞。
fn build_cpal_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: AudioRingConsumer,
) -> Result<cpal::Stream, AudioSpawnError> {
    let stream = device.build_output_stream(
        config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            // cpal 回调：从 ring buffer 读取，空了就填静音
            let written = consumer.pop_into(data);
            if written < data.len() {
                for sample in &mut data[written..] {
                    *sample = 0.0;
                }
            }
        },
        |err| {
            tracing::error!("cpal 音频流错误: {}", err);
        },
        None,
    )?;
    Ok(stream)
}

/// Worker 线程主循环 — 处理模型加载请求。
fn run_worker_thread(
    engine: Arc<Mutex<AudioEngine>>,
    worker_rx: Receiver<WorkerMessage>,
    sample_rate: u32,
) {
    while let Ok(msg) = worker_rx.recv() {
        match msg {
            WorkerMessage::LoadModel {
                doc,
                soundfont_paths,
            } => {
                let result = prepare_model::run_worker(doc, soundfont_paths, sample_rate);
                match result {
                    WorkerResult::ModelPrepared { model, soundfonts } => {
                        // 先加锁设置 soundfonts，释放后再加锁加载模型
                        // 避免在 prepare_model 期间持有锁阻塞 renderer
                        {
                            let mut eng = engine.lock().unwrap();
                            eng.set_soundfonts(soundfonts);
                        }
                        {
                            let mut eng = engine.lock().unwrap();
                            eng.load_model(model);
                        }
                        tracing::info!("音频引擎: 模型已加载 (worker 线程)");
                    }
                    WorkerResult::Error(e) => {
                        tracing::error!("音频引擎 worker 错误: {}", e);
                    }
                }
            }
            WorkerMessage::Shutdown => break,
        }
    }
    tracing::info!("音频 worker 线程已优雅退出");
}

impl CpalAudioHandle {
    /// 加载 MIDI 文档和音色库（在 worker 线程异步执行）。
    pub fn load_model(
        &self,
        doc: Arc<MidiDocument>,
        soundfont_paths: Vec<PathBuf>,
    ) {
        let _ = self.worker_tx.send(WorkerMessage::LoadModel {
            doc,
            soundfont_paths,
        });
    }

    /// 开始播放。
    pub fn play(&self) {
        let _ = self.cmd_tx.send(AudioCommand::Play);
    }

    /// 暂停播放。
    pub fn pause(&self) {
        let _ = self.cmd_tx.send(AudioCommand::Pause);
    }

    /// 停止播放并回到起点。
    pub fn stop(&self) {
        let _ = self.cmd_tx.send(AudioCommand::Stop);
    }

    /// Seek 到指定 tick。
    pub fn seek_tick(&self, tick: u32) {
        let _ = self.cmd_tx.send(AudioCommand::SeekTick(tick));
    }

    /// Seek 到指定 sample。
    pub fn seek_sample(&self, sample: u64) {
        let _ = self.cmd_tx.send(AudioCommand::SeekSample(sample));
    }

    /// 试听 NoteOn。
    pub fn note_on(&self, channel: u8, key: u8, velocity: u8) {
        let _ = self.cmd_tx.send(AudioCommand::NoteOn {
            channel,
            key,
            velocity,
        });
    }

    /// 试听 NoteOff。
    pub fn note_off(&self, channel: u8, key: u8) {
        let _ = self.cmd_tx.send(AudioCommand::NoteOff { channel, key });
    }

    /// 发送 CC。
    pub fn control_change(&self, channel: u8, controller: u8, value: u8) {
        let _ = self.cmd_tx.send(AudioCommand::ControlChange {
            channel,
            controller,
            value,
        });
    }

    /// 发送 ProgramChange。
    pub fn program_change(&self, channel: u8, program: u8) {
        let _ = self.cmd_tx.send(AudioCommand::ProgramChange {
            channel,
            program,
        });
    }

    /// 发送 PitchBend。
    pub fn pitch_bend(&self, channel: u8, value: f32) {
        let _ = self.cmd_tx.send(AudioCommand::PitchBend { channel, value });
    }

    /// 全部音符关闭。
    pub fn all_notes_off(&self) {
        let _ = self.cmd_tx.send(AudioCommand::AllNotesOff);
    }

    /// 重置所有控制器。
    pub fn reset_all(&self) {
        let _ = self.cmd_tx.send(AudioCommand::ResetAll);
    }

    /// 获取最新状态快照。
    pub fn state(&self) -> Option<EngineStateSnapshot> {
        self.state_rx.try_recv().ok()
    }

    /// 获取当前播放位置（tick）。
    pub fn position_tick(&self) -> f64 {
        self.engine.lock().unwrap().position_tick()
    }

    /// 获取当前播放位置（sample）。
    pub fn position_samples(&self) -> u64 {
        self.engine.lock().unwrap().position_samples()
    }

    /// 获取播放状态。
    pub fn play_state(&self) -> PlayState {
        self.engine.lock().unwrap().play_state
    }
}

impl Drop for CpalAudioHandle {
    fn drop(&mut self) {
        // 1. 发送 shutdown 信号
        self.shutdown_flag.store(true, Ordering::Relaxed);
        let _ = self.cmd_tx.send(AudioCommand::Shutdown);
        let _ = self.worker_tx.send(WorkerMessage::Shutdown);

        // 2. 等待 renderer 线程结束（最多 2 秒超时）
        if let Some(handle) = self.renderer_handle.take() {
            let start = std::time::Instant::now();
            while handle.is_alive() && start.elapsed() < std::time::Duration::from_secs(2) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if handle.is_alive() {
                tracing::warn!("音频渲染线程未在 2 秒内退出，强制继续");
            }
        }

        // 3. 等待 worker 线程结束（最多 1 秒超时）
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }

        // 4. 停止 cpal 流
        let _ = self.stream.pause();

        tracing::info!("CpalAudioHandle 已清理");
    }
}

/// 音频启动错误。
#[derive(Debug)]
pub enum AudioSpawnError {
    NoOutputDevice,
    NoSupportedConfig,
    DeviceError(String),
    CpalError(cpal::BuildStreamError),
    StreamError(cpal::PlayStreamError),
    ThreadError(std::io::Error),
}

impl From<cpal::BuildStreamError> for AudioSpawnError {
    fn from(e: cpal::BuildStreamError) -> Self {
        Self::CpalError(e)
    }
}

impl From<cpal::PlayStreamError> for AudioSpawnError {
    fn from(e: cpal::PlayStreamError) -> Self {
        Self::StreamError(e)
    }
}

impl From<cpal::DevicesError> for AudioSpawnError {
    fn from(_: cpal::DevicesError) -> Self {
        Self::NoOutputDevice
    }
}

impl From<std::io::Error> for AudioSpawnError {
    fn from(e: std::io::Error) -> Self {
        Self::ThreadError(e)
    }
}

impl std::fmt::Display for AudioSpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOutputDevice => write!(f, "找不到音频输出设备"),
            Self::NoSupportedConfig => write!(f, "不支持的音频配置"),
            Self::DeviceError(e) => write!(f, "设备错误: {}", e),
            Self::CpalError(e) => write!(f, "cpal 构建流错误: {}", e),
            Self::StreamError(e) => write!(f, "cpal 播放流错误: {}", e),
            Self::ThreadError(e) => write!(f, "线程启动错误: {}", e),
        }
    }
}

impl std::error::Error for AudioSpawnError {}

/// 辅助 trait 用于检查线程是否存活
trait ThreadAlive {
    fn is_alive(&self) -> bool;
}

impl ThreadAlive for JoinHandle<()> {
    fn is_alive(&self) -> bool {
        !self.is_finished()
    }
}
