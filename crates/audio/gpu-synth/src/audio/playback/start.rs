use super::config::negotiate_config;
use super::*;

impl AudioPlayback {
    /// Starts a realtime playback session.
    ///
    /// `nps_gate_enabled`：LGS 防爆闸开关（发送端软 NPS 闸；关闭时恒放行）。
    pub fn start(
        mut synth: GpuSynth,
        device: Option<cpal::Device>,
        nps_gate_enabled: bool,
    ) -> Result<Self, SynthError> {
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
        let (stream_config, resample_needed) =
            negotiate_config(&device, engine_rate, channels, block)?;
        let device_rate = stream_config.sample_rate.0;
        let needs_resample = resample_needed || device_rate != engine_rate;

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (event_tx, event_rx) = mpsc::channel::<StampedEvent>();
        let (stream_tx, stream_rx) = mpsc::channel::<Vec<crate::midi::TimedEvent>>();
        let (ctrl_tx, ctrl_rx) = mpsc::channel::<PlaybackControl>();
        let (sample_tx, sample_rx) = mpsc::sync_channel::<Vec<f32>>(32);
        let stop_flag = Arc::new(AtomicBool::new(false));
        // REND-016 #139：发送端准入限速状态（渲染线程发布级别，发送端应用；
        // `nps_gate_enabled` = 用户「LGS 防爆闸」开关）。
        let admission = Arc::new(AdmissionState::new(nps_gate_enabled));

        // Stats shared between the callback, the render thread and the caller.
        let stats = PlaybackStatsReader {
            samples: Arc::new(AtomicI64::new(0)),
            last_request_samples: Arc::new(AtomicI64::new(0)),
            last_samples_after_read: Arc::new(AtomicI64::new(0)),
            render_time: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
            render_time_head: Arc::new(AtomicU64::new(0)),
            render_size: Arc::new(AtomicU64::new((block * channels) as u64)),
            voice_count: Arc::new(AtomicU64::new(0)),
            underruns: Arc::new(AtomicU64::new(0)),
            dropped_note_ons: Arc::new(AtomicU64::new(0)),
            governor_level: Arc::new(AtomicU64::new(0)),
        };
        let cb_stats = stats.clone();

        // 注意：cpal 0.15 的 `Stream` 在 Windows 上是 `!Send`（携带
        // `NotSendSyncAcrossAllPlatforms`），不能在 `Send` 结构上跨线程移动。
        // 与 xsynth-realtime 相同，音频流在下方 start 末尾的「stream owner」
        // 线程内部创建并持有，`AudioPlayback` 只保留其 `JoinHandle`。

        // Pre-fill the queue with a few silent blocks BEFORE the render
        // thread and the audio callback race each other: the callback can
        // fire as soon as `stream.play()` above returns, and the first real
        // (dense) block takes tens of ms to render - without this cushion
        // the opening of black-MIDI underruns. The blocks are silence (no
        // events are loaded yet) and stream out at the normal pace. 8
        // blocks (~170ms at 2048/48k) covers the first dense block render
        // plus the render thread's catch-up burst.
        {
            let mut warm_buf = vec![0.0f32; block * channels];
            let mut warm_resampler = SincResampler::new(engine_rate, device_rate, channels);
            for _ in 0..8 {
                let _ = synth.render_block(&mut warm_buf);
                let out = if needs_resample {
                    warm_resampler.process(&warm_buf)
                } else {
                    warm_buf.clone()
                };
                stats.samples.fetch_add(out.len() as i64, Ordering::SeqCst);
                sample_tx.send(out).map_err(|_| {
                    SynthError::Gpu("queue prefill: audio queue closed during startup".into())
                })?;
            }
        }

        // Render thread: owns the engine and renders continuously. The
        // cadence is the block's wall-clock duration * 90% so the thread can
        // catch up when a block is slow. If it is more than 10% ahead of the
        // consumer it sleeps; otherwise it keeps rendering. Blocks are never
        // dropped: if the queue is full we wait (the consumer is draining).
        let thread_stop = stop_flag.clone();
        let thread_stats = stats.clone();
        let thread_admission = Arc::clone(&admission);
        let thread = thread::Builder::new()
            .name("lumino-gpu-synth-render".into())
            .spawn(move || {
                let mut synth = synth;
                let mut buf = vec![0.0f32; block * channels];
                let mut resampler = SincResampler::new(engine_rate, device_rate, channels);
                let mut last_err = false;
                // Max allowed render time per block: 90% of realtime so the
                // thread runs slightly ahead and the queue accumulates a
                // cushion that absorbs peak blocks (dense black-MIDI). The
                // queue's `try_send` wait throttles when we run too far
                // ahead. NOTE: based on `block` (one frame, all channels) —
                // using `block * channels` would double the budget.
                let delay = Duration::from_secs_f64(block as f64 / engine_rate.max(1) as f64 * 0.9);

                // If a full event stream is supplied, the engine consumes it
                // internally by `global_frame` (no per-event channel traffic)
                // - the only way to keep up with dense black-MIDI.
                let mut has_stream = false;
                // REND-016 #139：负载治理器（级别 → 新鲜 NoteOn 准入预算）。
                let mut governor = drain::Governor::new();

                loop {
                    // Accept an event stream (usually once, at startup).
                    if let Ok(events) = stream_rx.try_recv() {
                        synth.set_events(events);
                        has_stream = true;
                    }
                    // REND-002 实时多端口：轻量控制命令（复位/踏板清理，不重开流）。
                    while let Ok(cmd) = ctrl_rx.try_recv() {
                        match cmd {
                            PlaybackControl::ResetState => synth.reset_channel_state(),
                            PlaybackControl::ReleaseAllDampers => synth.release_all_dampers(),
                        }
                    }
                    // REND-016 #139：有界 drain + 过期 NoteOn 丢弃（墙钟时间闸）+
                    // 治理器准入预算。防止大块渲染结束后一次性注入上万积压事件
                    // → 下一块复音更高 → 级联大块 → 队列耗尽（欠载无声）。
                    let drain_outcome = drain::drain_events(
                        &event_rx,
                        Instant::now(),
                        drain::event_deadline(block, engine_rate),
                        &governor,
                        &thread_stats,
                        |ch, ev| synth.send_event(ch, ev),
                    );
                    if thread_stop.load(Ordering::Relaxed) || stop_rx.try_recv().is_ok() {
                        break;
                    }

                    // When a full stream is loaded, stop the render thread
                    // once the stream is exhausted (plus a decay tail) so the
                    // playback ends on its own instead of idling forever.
                    if has_stream {
                        let done = synth.stream_exhausted();
                        if done {
                            break;
                        }
                    }

                    // No explicit backpressure loop here: the bounded queue
                    // itself is the throttle. The cadence sleep below paces
                    // rendering at 90% of realtime (so we stay slightly
                    // ahead), and when the queue fills the `try_send` wait
                    // below naturally slows us to the consumer's pace. An
                    // explicit "samples > requested * k" check would compare
                    // one block (~8k samples) against a single callback
                    // request (~1k samples) and sleep after every block,
                    // keeping the queue nearly empty - the cause of the
                    // periodic underruns.

                    let start = Instant::now();
                    if let Err(e) = synth.render_block(&mut buf) {
                        // Never die silently: a wedged GPU surfaces here every
                        // block; print it once so the freeze is diagnosable
                        // instead of looking like a hung process.
                        if !last_err {
                            tracing::error!("[render] block error: {e}");
                            eprintln!("[render] block error: {e}");
                            last_err = true;
                        }
                        std::thread::sleep(delay / 10);
                        continue;
                    }
                    last_err = false;

                    // NOTE: lookahead sample pre-upload is NOT done here.
                    // Resampling a large SF2 sample takes ~300 ms no matter
                    // how it is chunked (the total work is fixed), so
                    // spreading it across blocks makes EVERY block slow
                    // instead of a few. The correct fix is `prewarm_midi_file`
                    // before playback (see examples/realtime_midi.rs); the
                    // engine keeps `prefetch_samples` for callers that want
                    // bounded incremental uploads.
                    thread_stats
                        .voice_count
                        .store(synth.voice_count() as u64, Ordering::Relaxed);

                    let out = if needs_resample {
                        resampler.process(&buf)
                    } else {
                        buf.clone()
                    };
                    thread_stats
                        .samples
                        .fetch_add(out.len() as i64, Ordering::SeqCst);

                    // Record the actual GPU/CPU render cost (render_block +
                    // resample), NOT including the cadence sleep below - the
                    // sleep is deliberate pacing, not render load.
                    let elapsed = start.elapsed().as_secs_f64();
                    let total = delay.as_secs_f64();
                    thread_stats.push_render_load(elapsed / total);

                    // REND-016 #139：治理器反馈（负载 EMA + 积压证据 → 级别/预算/
                    // 运行时声部上限）。收缩声部上限让块渲染成本 ∝ 声部数地下降，
                    // 是消除"卡顿期间持续欠载静音"的关键手段。
                    if let Some(level) =
                        governor.observe(elapsed / total, drain_outcome.emergency_evidence)
                    {
                        let voices = synth.voice_count();
                        let applied_limit = synth.set_runtime_voice_limit(level.voice_limit());
                        tracing::warn!(
                            "[GOVERNOR] 级别切换 -> L{}（load {:.2}, voices {}, 运行时声部上限 {}，本块 drain {} 条，过期 {}，预算 {}）",
                            level as u8,
                            elapsed / total,
                            voices,
                            applied_limit,
                            drain_outcome.processed,
                            drain_outcome.dropped_expired,
                            drain_outcome.dropped_budget
                        );
                        eprintln!(
                            "[GOVERNOR] 级别切换 -> L{}（load {:.2}, voices {}, 运行时声部上限 {}，本块 drain {} 条，过期 {}，预算 {}）",
                            level as u8,
                            elapsed / total,
                            voices,
                            applied_limit,
                            drain_outcome.processed,
                            drain_outcome.dropped_expired,
                            drain_outcome.dropped_budget
                        );
                    }
                    thread_stats.set_governor_level(governor.level() as u64);
                    // REND-016 #139：向发送端发布级别（NoteOn 准入限速）。
                    thread_admission.set_level(governor.level() as u64);

                    // Push without dropping: wait while the queue is full.
                    // The wait below is backpressure (the consumer is
                    // draining), NOT render load - so the render-load
                    // percentage is recorded BEFORE it, once per block,
                    // from the actual `render_block` cost only.
                    loop {
                        if sample_tx.try_send(out.clone()).is_ok() {
                            break;
                        }
                        if thread_stop.load(Ordering::Relaxed) || stop_rx.try_recv().is_ok() {
                            return;
                        }
                        std::thread::sleep(delay / 10);
                    }

                    // Sleep until the next cadence point (90% of realtime) to
                    // play in real time - UNLESS the queue is below the
                    // target cushion (about two blocks buffered): then render
                    // back-to-back to refill it so peak blocks never starve
                    // the audio callback.
                    let now = Instant::now();
                    let end = start + delay;
                    let cushion = (block * channels * 2) as i64;
                    if thread_stats.samples.load(Ordering::SeqCst) >= cushion && end > now {
                        std::thread::sleep(end - now);
                    }
                }
            })
            .map_err(SynthError::Io)?;

        // ── stream owner 线程：在「线程内部」创建 cpal Stream ──
        // 绝不在 Send 结构上跨线程移动 !Send 的 `Stream`；线程内建流、播放、
        // 然后持活直到 `stop_flag` 置位（Stream 析构即停止音频回调）。
        let (stream_tx_result, stream_rx_result) = mpsc::channel::<Result<(), SynthError>>();
        let owner_stop = stop_flag.clone();
        let stream_owner = thread::Builder::new()
            .name("lumino-gpu-synth-stream-owner".into())
            .spawn(move || {
                let err_fn = |e| {
                    tracing::error!("lumino-gpu-synth playback error: {e}");
                    eprintln!("lumino-gpu-synth playback error: {e}");
                };
                let mut next_block: Vec<f32> = Vec::new();
                let mut next_pos = 0usize;
                let stream = match device.build_output_stream(
                    &stream_config,
                    move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                        cb_stats
                            .last_request_samples
                            .store(data.len() as i64, Ordering::SeqCst);
                        let mut i = 0;
                        while i < data.len() {
                            if next_pos >= next_block.len() {
                                match sample_rx.try_recv() {
                                    Ok(b) => {
                                        next_block = b;
                                        next_pos = 0;
                                    }
                                    Err(_) => {
                                        data[i..].fill(0.0);
                                        cb_stats.underruns.fetch_add(1, Ordering::Relaxed);
                                        let ms = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis()
                                            as i64;
                                        let prev = LAST_UD_LOG.fetch_max(ms, Ordering::Relaxed);
                                        if ms - prev > 500 {
                                            tracing::warn!(
                                                "[UNDERRUN] queue empty (total: {})",
                                                cb_stats.underruns.load(Ordering::Relaxed)
                                            );
                                            eprintln!(
                                                "[UNDERRUN] queue empty (total: {})",
                                                cb_stats.underruns.load(Ordering::Relaxed)
                                            );
                                        }
                                        break;
                                    }
                                }
                            } else {
                                let n = (next_block.len() - next_pos).min(data.len() - i);
                                data[i..i + n].copy_from_slice(&next_block[next_pos..next_pos + n]);
                                i += n;
                                next_pos += n;
                            }
                        }
                        cb_stats.samples.fetch_sub(i as i64, Ordering::SeqCst);
                        cb_stats
                            .last_samples_after_read
                            .store(cb_stats.samples.load(Ordering::SeqCst), Ordering::Relaxed);
                    },
                    err_fn,
                    None,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = stream_tx_result
                            .send(Err(SynthError::Gpu(format!("audio stream: {e}"))));
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    let _ = stream_tx_result
                        .send(Err(SynthError::Gpu(format!("audio stream play: {e}"))));
                    return;
                }
                // 通知主线程流已就绪；随后保持 Stream 存活直到停止。
                let _ = stream_tx_result.send(Ok(()));
                let _stream = stream;
                while !owner_stop.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            })
            .map_err(SynthError::Io)?;

        match stream_rx_result.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(SynthError::Gpu(
                    "audio stream thread terminated during startup".into(),
                ));
            }
        }

        Ok(Self {
            stop_flag,
            stop_tx: Some(stop_tx),
            event_tx: Some(event_tx),
            admission,
            stream_tx: Some(stream_tx),
            ctrl_tx: Some(ctrl_tx),
            thread: Some(thread),
            sample_rate: device_rate,
            engine_rate,
            stats,
            _stream_owner: Some(stream_owner),
        })
    }
}
