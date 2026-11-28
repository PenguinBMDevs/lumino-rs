use super::config::negotiate_config;
use super::*;

/// 采样队列最大缓冲时长（ms）：按时间封顶延迟——固定 32 块在块大时缓冲
/// 过久（8192@64k ≈ 4.1s，"没法听"）。见队列创建处。
const QUEUE_MAX_LATENCY_MS: u64 = 600;
/// 队列深度下限（欠载缓冲：至少几块才能吸收峰值块）。
const QUEUE_MIN_BLOCKS: usize = 4;
/// 队列深度上限（原固定值；512/1024 行为不变）。
const QUEUE_MAX_BLOCKS: usize = 32;

mod guard;
mod recycler;
mod render;
mod stream_owner;

use guard::StartupThreadGuard;
use render::spawn as spawn_render_thread;
use stream_owner::spawn as spawn_stream_owner;

/// 设备与流协商结果：7 项启动期常量，一次性产出后按原顺序交给各阶段。
struct StartupSetup {
    engine_rate: u32,
    channels: usize,
    block: usize,
    device: cpal::Device,
    stream_config: cpal::StreamConfig,
    device_rate: u32,
    needs_resample: bool,
}

/// 启动期共享状态：通道与原子量按原顺序集中装配。
/// 字段偏多是因为回调、渲染线程和调用方三方各持有独立的发送端与计数，
/// 集中在一起才能让创建顺序与拆分前逐项一致；纯启动期 DTO，不进实时路径。
struct SharedState {
    stop_tx: mpsc::Sender<()>,
    stop_rx: mpsc::Receiver<()>,
    event_tx: mpsc::Sender<StampedEvent>,
    event_rx: mpsc::Receiver<StampedEvent>,
    stream_tx: mpsc::Sender<Vec<crate::midi::TimedEvent>>,
    stream_rx: mpsc::Receiver<Vec<crate::midi::TimedEvent>>,
    ctrl_tx: mpsc::Sender<PlaybackControl>,
    ctrl_rx: mpsc::Receiver<PlaybackControl>,
    sample_tx: mpsc::SyncSender<Vec<f32>>,
    sample_rx: mpsc::Receiver<Vec<f32>>,
    stop_flag: Arc<AtomicBool>,
    admission: Arc<AdmissionState>,
    stats: PlaybackStatsReader,
    cb_stats: PlaybackStatsReader,
    queue_blocks: usize,
    block: usize,
}

impl SharedState {
    fn new(
        engine_rate: u32,
        channels: usize,
        block: usize,
        device_rate: u32,
        needs_resample: bool,
        nps_gate_enabled: bool,
    ) -> Self {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (event_tx, event_rx) = mpsc::channel::<StampedEvent>();
        let (stream_tx, stream_rx) = mpsc::channel::<Vec<crate::midi::TimedEvent>>();
        let (ctrl_tx, ctrl_rx) = mpsc::channel::<PlaybackControl>();
        // REND-016 #139：采样队列深度按**时间**封顶（≤600ms 音频）——固定
        // 32 块在块大时缓冲过久（8192@64k ≈ 4.1s 延迟，现场"没法听"）；
        // 512/1024 下仍为上限 32 块，行为不变。
        let block_period_ms = (block as u64).saturating_mul(1000) / u64::from(engine_rate.max(1));
        let queue_blocks = (QUEUE_MAX_LATENCY_MS / block_period_ms.max(1))
            .clamp(QUEUE_MIN_BLOCKS as u64, QUEUE_MAX_BLOCKS as u64)
            as usize;
        let (sample_tx, sample_rx) = mpsc::sync_channel::<Vec<f32>>(queue_blocks);
        let stop_flag = Arc::new(AtomicBool::new(false));
        // REND-016 #139：发送端准入限速状态（渲染线程发布级别，发送端应用；
        // `nps_gate_enabled` = 用户「LGS 防爆闸」开关）。
        let admission = Arc::new(AdmissionState::new(nps_gate_enabled));
        // REF-LGS-016：这里曾有 `flush_epoch`（过载时丢弃已渲染的旧音频）。`7bb878b9`
        // 把 REND-016 的入口重同步改成「宁糊不切」后就不再递增它，机制已死；
        // 2026-11-01 删除。**不要重新引入丢弃队列音频的路径**：现场验收里它造成
        // 可听跳跃，且会被懒重采样的慢块误触发（详见 REND-016 实施记录）。

        // 队列样本计数（交错样本）的换算基准——重采样时队列块按
        // 设备率计数，否则按引擎率计数；供 UI 把 samples 换算成时间。
        let queue_rate = if needs_resample {
            device_rate
        } else {
            engine_rate
        };

        // Stats shared between the callback, the render thread and the caller.
        let stats = PlaybackStatsReader {
            samples: Arc::new(AtomicI64::new(0)),
            last_request_samples: Arc::new(AtomicI64::new(0)),
            last_samples_after_read: Arc::new(AtomicI64::new(0)),
            render_time: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
            render_time_head: Arc::new(AtomicU64::new(0)),
            render_size: Arc::new(AtomicU64::new((block * channels) as u64)),
            samples_per_second: u64::from(queue_rate) * channels as u64,
            voice_count: Arc::new(AtomicU64::new(0)),
            underruns: Arc::new(AtomicU64::new(0)),
            playing: Arc::new(AtomicBool::new(false)),
            dropped_note_ons: Arc::new(AtomicU64::new(0)),
            governor_level: Arc::new(AtomicU64::new(0)),
            sample_gpu_bytes: Arc::new(AtomicU64::new(0)),
            sample_cpu_bytes: Arc::new(AtomicU64::new(0)),
            sample_resample_bytes: Arc::new(AtomicU64::new(0)),
        };
        let cb_stats = stats.clone();
        Self {
            stop_tx,
            stop_rx,
            event_tx,
            event_rx,
            stream_tx,
            stream_rx,
            ctrl_tx,
            ctrl_rx,
            sample_tx,
            sample_rx,
            stop_flag,
            admission,
            stats,
            cb_stats,
            queue_blocks,
            block,
        }
    }
}

impl StartupSetup {
    /// 预填静音块：配置取自本结构体，共享通道取自 [`SharedState`]，
    /// 计算式与 `?` 传播位置与原内联代码一致。
    fn prefill(&self, synth: &mut GpuSynth, shared: &SharedState) -> Result<(), SynthError> {
        // Pre-fill the queue with a few silent blocks BEFORE the render
        // thread and the audio callback race each other: the callback can
        // fire as soon as `stream.play()` above returns, and the first real
        // (dense) block takes tens of ms to render - without this cushion
        // the opening of black-MIDI underruns. The blocks are silence (no
        // events are loaded yet) and stream out at the normal pace. 8
        // blocks (~170ms at 2048/48k) covers the first dense block render
        // plus the render thread's catch-up burst.
        {
            let mut warm_buf = vec![0.0f32; shared.block * self.channels];
            let mut warm_resampler =
                SincResampler::new(self.engine_rate, self.device_rate, self.channels);
            for _ in 0..shared.queue_blocks.min(8) {
                let _ = synth.render_block(&mut warm_buf);
                let out = if self.needs_resample {
                    warm_resampler.process(&warm_buf)
                } else {
                    warm_buf.clone()
                };
                shared
                    .stats
                    .samples
                    .fetch_add(out.len() as i64, Ordering::SeqCst);
                shared.sample_tx.send(out).map_err(|_| {
                    SynthError::Gpu("queue prefill: audio queue closed during startup".into())
                })?;
            }
        }
        Ok(())
    }
}

/// 等待 stream owner 报告建流结果；错误与通道关闭的返回路径保持原样。
fn await_stream_ready(rx: &mpsc::Receiver<Result<(), SynthError>>) -> Result<(), SynthError> {
    match rx.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(SynthError::Gpu(
            "audio stream thread terminated during startup".into(),
        )),
    }
}

/// 设备选择 + 采样率协商：错误在调用点经 `?` 原样上抛。
fn resolve_setup(
    synth: &GpuSynth,
    device: Option<cpal::Device>,
) -> Result<StartupSetup, SynthError> {
    let engine_rate = synth.config().sample_rate;
    let channels = synth.config().channels.channel_count();
    let block = synth.config().block_size;

    // 优先使用调用方解析出的指定输出设备；否则回退到系统默认输出设备。
    let device = match device {
        Some(d) => d,
        None => cpal::default_host()
            .default_output_device()
            .ok_or_else(|| SynthError::Gpu("no default audio output device".into()))?,
    };

    // Negotiate: prefer the engine rate, else the device default, and
    // remember which we got so the render thread can resample.
    let (stream_config, resample_needed) = negotiate_config(&device, engine_rate, channels, block)?;
    let device_rate = stream_config.sample_rate.0;
    let needs_resample = resample_needed || device_rate != engine_rate;
    Ok(StartupSetup {
        engine_rate,
        channels,
        block,
        device,
        stream_config,
        device_rate,
        needs_resample,
    })
}

impl AudioPlayback {
    /// Starts a realtime playback session.
    ///
    /// `nps_gate_enabled`：LGS 防爆闸开关（发送端软 NPS 闸；关闭时恒放行）。
    pub fn start(
        mut synth: GpuSynth,
        device: Option<cpal::Device>,
        nps_gate_enabled: bool,
    ) -> Result<Self, SynthError> {
        let setup = resolve_setup(&synth, device)?;
        let shared = SharedState::new(
            setup.engine_rate,
            setup.channels,
            setup.block,
            setup.device_rate,
            setup.needs_resample,
            nps_gate_enabled,
        );
        setup.prefill(&mut synth, &shared)?;

        let (recycle_tx, recycle_rx) = mpsc::sync_channel::<Vec<f32>>(shared.queue_blocks + 2);

        let render_thread = spawn_render_thread(render::RenderArgs {
            synth,
            block: setup.block,
            channels: setup.channels,
            engine_rate: setup.engine_rate,
            device_rate: setup.device_rate,
            needs_resample: setup.needs_resample,
            stop_flag: shared.stop_flag.clone(),
            stop_rx: shared.stop_rx,
            stats: shared.stats.clone(),
            admission: Arc::clone(&shared.admission),
            stream_rx: shared.stream_rx,
            ctrl_rx: shared.ctrl_rx,
            event_rx: shared.event_rx,
            sample_tx: shared.sample_tx,
            recycle_rx,
        })
        .map_err(SynthError::Io)?;
        let mut startup_guard = StartupThreadGuard::new(
            shared.stop_flag.clone(),
            shared.stop_tx.clone(),
            render_thread,
        );

        // 注意：cpal 0.15 的 `Stream` 在 Windows 上是 `!Send`（携带
        // `NotSendSyncAcrossAllPlatforms`），不能在 `Send` 结构上跨线程移动。
        // 与 xsynth-realtime 相同，音频流在下方 start 末尾的「stream owner」
        // 线程内部创建并持有，`AudioPlayback` 只保留其 `JoinHandle`。

        // ── stream owner 线程：在「线程内部」创建 cpal Stream ──
        // 绝不在 Send 结构上跨线程移动 !Send 的 `Stream`；线程内建流、播放、
        // 然后持活直到 `stop_flag` 置位（Stream 析构即停止音频回调）。
        let (stream_tx_result, stream_rx_result) = mpsc::channel::<Result<(), SynthError>>();
        let stream_owner = spawn_stream_owner(stream_owner::StreamOwnerArgs {
            device: setup.device,
            stream_config: setup.stream_config,
            stop_flag: shared.stop_flag.clone(),
            stats: shared.cb_stats,
            queue_blocks: shared.queue_blocks,
            recycle_tx,
            sample_rx: shared.sample_rx,
            ready_tx: stream_tx_result,
        })
        .map_err(SynthError::Io)?;
        startup_guard.attach_stream_owner(stream_owner);

        await_stream_ready(&stream_rx_result)?;
        let (thread, stream_owner) = startup_guard.disarm();
        Ok(Self {
            stop_flag: shared.stop_flag,
            stop_tx: Some(shared.stop_tx),
            event_tx: Some(shared.event_tx),
            admission: shared.admission,
            stream_tx: Some(shared.stream_tx),
            ctrl_tx: Some(shared.ctrl_tx),
            thread,
            sample_rate: setup.device_rate,
            engine_rate: setup.engine_rate,
            stats: shared.stats,
            _stream_owner: stream_owner,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::recycler::CallbackBufferRecycler;
    use super::*;

    fn spawn_stoppable(stop: Arc<AtomicBool>, stopped: mpsc::Sender<()>) -> JoinHandle<()> {
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(1));
            }
            let _ = stopped.send(());
        })
    }

    #[test]
    fn test_startup_guard_stops_render_thread_without_stream_owner() {
        let stop = Arc::new(AtomicBool::new(false));
        let (stop_tx, _stop_rx) = mpsc::channel();
        let (stopped_tx, stopped_rx) = mpsc::channel();
        let render = spawn_stoppable(stop.clone(), stopped_tx);
        let guard = StartupThreadGuard::new(stop, stop_tx, render);

        drop(guard);

        stopped_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("启动失败应停止并回收渲染线程");
    }

    #[test]
    fn test_startup_guard_stops_render_and_stream_owner_threads() {
        let stop = Arc::new(AtomicBool::new(false));
        let (stop_tx, _stop_rx) = mpsc::channel();
        let (stopped_tx, stopped_rx) = mpsc::channel();
        let render = spawn_stoppable(stop.clone(), stopped_tx.clone());
        let owner = spawn_stoppable(stop.clone(), stopped_tx);
        let mut guard = StartupThreadGuard::new(stop, stop_tx, render);
        guard.attach_stream_owner(owner);

        drop(guard);

        for _ in 0..2 {
            stopped_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("启动失败应停止并回收全部已启动线程");
        }
    }

    #[test]
    fn test_callback_recycler_defers_when_channel_unavailable() {
        let (tx, _rx) = mpsc::sync_channel(0);
        let mut recycler = CallbackBufferRecycler::new(tx, 1);

        recycler.recycle(vec![1.0, 2.0]);
        recycler.recycle(vec![3.0, 4.0]);
        recycler.recycle(vec![5.0, 6.0]);
        assert_eq!(recycler.deferred.len(), 2);
        assert_eq!(recycler.deferred[0], vec![1.0, 2.0]);
        assert_eq!(recycler.deferred[1], vec![3.0, 4.0]);

        recycler.flush();

        assert_eq!(recycler.deferred.len(), 2);
    }
}
