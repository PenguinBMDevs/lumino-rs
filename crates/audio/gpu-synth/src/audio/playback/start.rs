use super::config::negotiate_config;
use super::*;

impl AudioPlayback {
    pub fn start(mut synth: GpuSynth, device: Option<cpal::Device>) -> Result<Self, SynthError> {
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
        let (event_tx, event_rx) = mpsc::channel::<(u8, MidiEvent)>();
        let (stream_tx, stream_rx) = mpsc::channel::<Vec<crate::midi::TimedEvent>>();
        let (sample_tx, sample_rx) = mpsc::sync_channel::<Vec<f32>>(32);
        let stop_flag = Arc::new(AtomicBool::new(false));

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
                sample_tx
                    .send(out)
                    .expect("queue prefill: audio queue must be empty at start");
            }
        }

        // Render thread: owns the engine and renders continuously. The
        // cadence is the block's wall-clock duration * 90% so the thread can
        // catch up when a block is slow. If it is more than 10% ahead of the
        // consumer it sleeps; otherwise it keeps rendering. Blocks are never
        // dropped: if the queue is full we wait (the consumer is draining).
        let thread_stop = stop_flag.clone();
        let thread_stats = stats.clone();
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

                loop {
                    // Accept an event stream (usually once, at startup).
                    if let Ok(events) = stream_rx.try_recv() {
                        synth.set_events(events);
                        has_stream = true;
                    }
                    // Drain pending MIDI events (non-blocking).
                    while let Ok((ch, ev)) = event_rx.try_recv() {
                        synth.send_event(ch, ev);
                    }
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
                let err_fn = |e| eprintln!("lumino-gpu-synth playback error: {e}");
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
            stream_tx: Some(stream_tx),
            thread: Some(thread),
            sample_rate: device_rate,
            engine_rate,
            stats,
            _stream_owner: Some(stream_owner),
        })
    }
}
