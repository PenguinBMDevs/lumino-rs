//! 渲染线程装配与循环。
//!
//! 从 `start` 内联搬迁，逐条保留原有阶段顺序：配置 → 治理器 → 输入接收 →
//! 有界 drain → 停止判定 → 渲染/统计 → 治理器反馈 → 入队与节拍睡眠。
//! 纯调度/管线拆分，不触碰任何 DSP 数学、缓冲长度或原子量语义。
//!
//! Render thread: owns the engine and renders continuously. The
//! cadence is the block's wall-clock duration * 90% so the thread can
//! catch up when a block is slow. If it is more than 10% ahead of the
//! consumer it sleeps; otherwise it keeps rendering. Blocks are never
//! dropped: if the queue is full we wait (the consumer is draining).

use super::super::*;

/// 渲染线程启动参数：15 项按原顺序取自 `start`，字段多是因为线程要独占
/// 引擎、队列两端、事件通道与统计；用参数结构体避免超长参数列表。
pub(super) struct RenderArgs {
    pub(super) synth: GpuSynth,
    pub(super) block: usize,
    pub(super) channels: usize,
    pub(super) engine_rate: u32,
    pub(super) device_rate: u32,
    pub(super) needs_resample: bool,
    pub(super) stop_flag: Arc<AtomicBool>,
    pub(super) stop_rx: mpsc::Receiver<()>,
    pub(super) stats: PlaybackStatsReader,
    pub(super) admission: Arc<AdmissionState>,
    pub(super) stream_rx: mpsc::Receiver<Vec<crate::midi::TimedEvent>>,
    pub(super) ctrl_rx: mpsc::Receiver<PlaybackControl>,
    pub(super) event_rx: mpsc::Receiver<StampedEvent>,
    pub(super) sample_tx: mpsc::SyncSender<Vec<f32>>,
    pub(super) recycle_rx: mpsc::Receiver<Vec<f32>>,
}

/// 渲染线程的不可变配置（6 项，启动时算一次）。
struct RenderConfig {
    block: usize,
    channels: usize,
    engine_rate: u32,
    needs_resample: bool,
    block_period: Duration,
    delay: Duration,
}

/// 渲染线程持有的通道端点（7 项，全部独占）。
struct RenderIo {
    stop_flag: Arc<AtomicBool>,
    stop_rx: mpsc::Receiver<()>,
    stream_rx: mpsc::Receiver<Vec<crate::midi::TimedEvent>>,
    ctrl_rx: mpsc::Receiver<PlaybackControl>,
    event_rx: mpsc::Receiver<StampedEvent>,
    sample_tx: mpsc::SyncSender<Vec<f32>>,
    recycle_rx: mpsc::Receiver<Vec<f32>>,
}

/// 渲染线程全部状态：字段多是因为独占的引擎、重采样器、治理器与三类
/// 通道需要在各阶段间传递；纯调度容器，不改变任何实时计算。
struct RenderState {
    cfg: RenderConfig,
    io: RenderIo,
    synth: GpuSynth,
    buf: Vec<f32>,
    resampler: SincResampler,
    stats: PlaybackStatsReader,
    admission: Arc<AdmissionState>,
    governor: drain::Governor,
    last_err: bool,
    mem_stat_countdown: u32,
    escalate_voices: usize,
    has_stream: bool,
    last_admission_dropped: u64,
    had_pressure: bool,
    clean_since_frame: Option<u64>,
}

impl RenderState {
    fn new(args: RenderArgs) -> Self {
        let RenderArgs {
            synth,
            block,
            channels,
            engine_rate,
            device_rate,
            needs_resample,
            stop_flag,
            stop_rx,
            stats,
            admission,
            stream_rx,
            ctrl_rx,
            event_rx,
            sample_tx,
            recycle_rx,
        } = args;
        let buf = vec![0.0f32; block * channels];
        let resampler = SincResampler::new(engine_rate, device_rate, channels);
        let last_err = false;
        // REF-LGS-013：采样内存快照的降频计数器（每 25 块发布一次）。
        let mem_stat_countdown = 0u32;
        // Max allowed per-block cost (drain + render) budget: 90% of
        // realtime so the thread runs slightly ahead and the queue
        // accumulates a cushion that absorbs peak blocks (dense
        // black-MIDI). `start` is captured at the top of the loop
        // (before drain) so this budget covers the FULL per-block
        // cost, not just `render_block`. The queue's `try_send` wait
        // throttles when we run too far ahead. NOTE: based on `block`
        // (one frame, all channels) — using `block * channels` would
        // double the budget.
        let block_period = Duration::from_secs_f64(block as f64 / engine_rate.max(1) as f64);
        let delay = block_period.mul_f64(0.9);
        // REND-016 #139：运行时声部上限随块周期缩放（每声部每块 ≈1µs，
        // 与块大小无关——512 块 8ms 预算下 12288 声部 ≈12.5ms 必然饱和）。
        let (overload_limit, emergency_limit) = drain::voice_limits_for(block_period);
        // 声部数即时升级阈值 = Overload 池满线（上限 + 50% 淡出槽）：
        // 只有真正超出可负担规模的大爆点才跳过证据/EMA 直入 Emergency
        // （小爆点走负载 EMA 的常规升级路径，不再被误触发）。
        let escalate_voices = overload_limit + overload_limit / 2;

        // If a full event stream is supplied, the engine consumes it
        // internally by `global_frame` (no per-event channel traffic)
        // - the only way to keep up with dense black-MIDI.
        let has_stream = false;
        // REND-016 #139：负载治理器（级别 → 新鲜 NoteOn 准入预算/声部上限）。
        let mut governor = drain::Governor::new();
        governor.set_voice_limits(overload_limit, emergency_limit);
        // 发送端闸丢弃计数快照（用于计算"丢弃压力"，驱动快速释放）。
        let last_admission_dropped = admission.dropped();
        // REND-016 #139：干净窗口跟踪——压力 true→false 的首帧记为
        // "风暴结束点"，出口重同步据此只清风暴期出生的余波。
        let had_pressure = false;
        let clean_since_frame: Option<u64> = None;

        Self {
            cfg: RenderConfig {
                block,
                channels,
                engine_rate,
                needs_resample,
                block_period,
                delay,
            },
            io: RenderIo {
                stop_flag,
                stop_rx,
                stream_rx,
                ctrl_rx,
                event_rx,
                sample_tx,
                recycle_rx,
            },
            synth,
            buf,
            resampler,
            stats,
            admission,
            governor,
            last_err,
            mem_stat_countdown,
            escalate_voices,
            has_stream,
            last_admission_dropped,
            had_pressure,
            clean_since_frame,
        }
    }

    /// 主循环：阶段调用顺序与拆分前逐行一致。
    fn run(&mut self) {
        loop {
            // REND-016 #139：块周期起点——必须**包含 drain/冲洗时间**：
            // ① 负载 EMA 计入 drain 成本（否则冲洗慢块不触发治理器，
            //    现场表现为"load 0.17 却持续缓速"且无任何日志）；
            // ② 节奏锚点不把 drain 耗时叠加在 delay 之上（旧口径每块
            //    周期 = drain + delay，天然慢于实时）。
            let start = Instant::now();
            self.accept_inputs();
            let (outcome, pressure) = self.drain_and_judge();
            if self.should_stop() {
                break;
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

            let Some((out, elapsed, total)) = self.render_and_meter(start) else {
                continue;
            };
            self.update_governor(elapsed, total, outcome, pressure);
            if !self.push_and_pace(start, out) {
                return;
            }
        }
    }

    /// 事件流与控制命令接收（startup 后通常只发生一次）。
    fn accept_inputs(&mut self) {
        // Accept an event stream (usually once, at startup).
        if let Ok(events) = self.io.stream_rx.try_recv() {
            self.synth.set_events(events);
            self.has_stream = true;
        }
        // REND-002 实时多端口：轻量控制命令（复位/踏板清理，不重开流）。
        while let Ok(cmd) = self.io.ctrl_rx.try_recv() {
            match cmd {
                PlaybackControl::ResetState => self.synth.reset_channel_state(),
                PlaybackControl::ReleaseAllDampers => self.synth.release_all_dampers(),
                PlaybackControl::ResetStats => self.stats.reset_counters(),
            }
        }
    }

    /// 有界 drain + 丢弃压力判定 + 干净窗口起点跟踪。
    fn drain_and_judge(&mut self) -> (drain::DrainOutcome, bool) {
        // REND-016 #139：有界 drain + 过期 NoteOn 丢弃（墙钟时间闸）+
        // 治理器准入预算。防止大块渲染结束后一次性注入上万积压事件
        // → 下一块复音更高 → 级联大块 → 队列耗尽（欠载无声）。
        let outcome = drain::drain_events(
            &self.io.event_rx,
            Instant::now(),
            drain::event_deadline(self.cfg.block, self.cfg.engine_rate),
            self.cfg.block_period,
            &self.governor,
            &self.stats,
            |ch, ev| self.synth.send_event(ch, ev),
        );
        // REND-016 #139：本块"丢弃压力"证据（发送端闸丢弃 + 渲染端
        // 过期/预算丢弃 + 紧急冲洗）。压力消失 → Governor 快速逐级
        // 放行，避免爆点后"等引擎静下来才放行"的人工静音尾巴。
        let dropped = self.admission.dropped();
        let pressure = dropped > self.last_admission_dropped
            || outcome.dropped_expired > 0
            || outcome.dropped_budget > 0
            || outcome.emergency_evidence;
        self.last_admission_dropped = dropped;
        // REND-016 #139：干净窗口起点（压力 true→false 的首帧）。
        if pressure {
            self.had_pressure = true;
            self.clean_since_frame = None;
        } else if self.had_pressure && self.clean_since_frame.is_none() {
            self.clean_since_frame = Some(self.synth.global_frame());
        }
        (outcome, pressure)
    }

    /// 停止判定：显式停止与事件流自然结束都清 `playing` 并终止循环。
    fn should_stop(&mut self) -> bool {
        if self.io.stop_flag.load(Ordering::Relaxed) || self.io.stop_rx.try_recv().is_ok() {
            self.stats.set_playing(false);
            return true;
        }

        // When a full stream is loaded, stop the render thread
        // once the stream is exhausted (plus a decay tail) so the
        // playback ends on its own instead of idling forever.
        if self.has_stream {
            let done = self.synth.stream_exhausted();
            if done {
                // 播放自然结束：队列随后排空，不能再算欠载。
                self.stats.set_playing(false);
                return true;
            }
        }
        false
    }

    /// 渲染一块并发布统计；返回 `None` 表示出错后需要 `continue`。
    fn render_and_meter(&mut self, start: Instant) -> Option<(Vec<f32>, f64, f64)> {
        if let Err(e) = self.synth.render_block(&mut self.buf) {
            // Never die silently: a wedged GPU surfaces here every
            // block; print it once so the freeze is diagnosable
            // instead of looking like a hung process.
            if !self.last_err {
                log_dual!(error, "[render] block error: {e}");
                self.last_err = true;
            }
            std::thread::sleep(self.cfg.delay / 10);
            return None;
        }
        self.last_err = false;

        // NOTE: lookahead sample pre-upload is NOT done here.
        // Resampling a large SF2 sample takes ~300 ms no matter
        // how it is chunked (the total work is fixed), so
        // spreading it across blocks makes EVERY block slow
        // instead of a few. The correct fix is `prewarm_midi_file`
        // before playback (see examples/realtime_midi.rs); the
        // engine keeps `prefetch_samples` for callers that want
        // bounded incremental uploads.
        self.stats
            .voice_count
            .store(self.synth.voice_count() as u64, Ordering::Relaxed);

        // REF-LGS-013：采样内存低频发布（每 25 块一次）。
        // `sample_memory_bytes` 会遍历样本/重采样缓存，不应当成
        // 每块热路径；状态栏是低频展示，25 块（约 0.25–0.5 s）
        // 的更新粒度足够。
        if self.mem_stat_countdown == 0 {
            let (gpu_bytes, cpu_bytes, resample_bytes) = self.synth.sample_memory_breakdown();
            self.stats
                .set_sample_memory(gpu_bytes, cpu_bytes, resample_bytes);
            self.mem_stat_countdown = 25;
        } else {
            self.mem_stat_countdown -= 1;
        }

        let out = if self.cfg.needs_resample {
            // 重采样器目前返回新 Vec；旧块仍在非实时渲染线程回收。
            let _ = self.io.recycle_rx.try_recv();
            self.resampler.process(&self.buf)
        } else {
            match self.io.recycle_rx.try_recv() {
                Ok(mut recycled) => {
                    recycled.clear();
                    recycled.extend_from_slice(&self.buf);
                    recycled
                }
                Err(_) => self.buf.clone(),
            }
        };
        self.stats
            .samples
            .fetch_add(out.len() as i64, Ordering::SeqCst);

        // Record the actual GPU/CPU render cost (render_block +
        // resample), NOT including the cadence sleep below - the
        // sleep is deliberate pacing, not render load.
        let elapsed = start.elapsed().as_secs_f64();
        let total = self.cfg.delay.as_secs_f64();
        self.stats.push_render_load(elapsed / total);
        Some((out, elapsed, total))
    }

    /// 治理器反馈、级别切换日志与级别发布。
    fn update_governor(
        &mut self,
        elapsed: f64,
        total: f64,
        outcome: drain::DrainOutcome,
        pressure: bool,
    ) {
        // REND-016 #139：治理器反馈（负载 EMA + 积压证据 → 级别/预算/
        // 运行时声部上限）。收缩声部上限让块渲染成本 ∝ 声部数地下降，
        // 是消除"卡顿期间持续欠载静音"的关键手段。
        let level_before = self.governor.level();
        let level_change = self
            .governor
            .observe(elapsed / total, outcome.emergency_evidence, pressure)
            .or_else(|| {
                // REND-016 #139：声部数即时升级——证据/EMA 要等 ~100ms，
                // 而声部爆掉的第一块就可能被 2~3 万声部拖成秒级巨块
                // （缓速根因）。超阈值时跳过等待直入 Emergency（下一块
                // 就 trim 到 8192）。
                if !matches!(self.governor.level(), drain::GovernorLevel::Emergency)
                    && self.synth.voice_count() > self.escalate_voices
                {
                    Some(self.governor.force_emergency())
                } else {
                    None
                }
            });
        if let Some(level) = level_change {
            // REND-016 #139：过载重同步——**入口**只降级保护（现场反馈：
            // "丢旧音频"造成可听跳跃、"淡出全部声部"造成暴力切断，比糊
            // 过去更难接受）；**出口**只清"风暴期出生"的余波（不动新段落、
            // 不动音频缓冲），避免风暴余波叠加到后面段落。
            let entering = matches!(level, drain::GovernorLevel::Emergency)
                && !matches!(level_before, drain::GovernorLevel::Emergency);
            let leaving = !matches!(level, drain::GovernorLevel::Emergency)
                && matches!(level_before, drain::GovernorLevel::Emergency);
            if entering {
                // 不丢弃音频、不淡出声部：仅施加运行时声部上限——
                // trim 保留最老的声部（正在演奏的乐句），爆点新音符
                // 被裁掉，音乐"糊过去"而不是被切断。
                log_dual!(
                    warn,
                    "[RESYNC] 入口降级保护：不切断、不跳段，仅施加声部上限（L{}）",
                    level as u8
                );
            } else if leaving {
                let cutoff = self.clean_since_frame.unwrap_or(self.synth.global_frame());
                let faded = self.synth.fade_voices_spawned_before(cutoff);
                log_dual!(
                    warn,
                    "[RESYNC] 出口清余波：淡出风暴期声部 {} 个（cutoff 帧 {}，L{}）",
                    faded,
                    cutoff,
                    level as u8
                );
            }
            let voices = self.synth.voice_count();
            let applied_limit = self
                .synth
                .set_runtime_voice_limit(self.governor.voice_limit());
            log_dual!(
                warn,
                "[GOVERNOR] 级别切换 -> L{}（load {:.2}, voices {}, 运行时声部上限 {}，本块 drain {} 条，过期 {}，预算 {}）",
                level as u8,
                elapsed / total,
                voices,
                applied_limit,
                outcome.processed,
                outcome.dropped_expired,
                outcome.dropped_budget
            );
        }
        self.stats.set_governor_level(self.governor.level() as u64);
        // REND-016 #139：向发送端发布级别（NoteOn 准入限速）。
        self.admission.set_level(self.governor.level() as u64);
    }

    /// 入队背压与节拍睡眠；返回 `false` 表示线程应直接退出。
    fn push_and_pace(&mut self, start: Instant, mut out: Vec<f32>) -> bool {
        // Push without dropping: wait while the queue is full.
        // The wait below is backpressure (the consumer is
        // draining), NOT render load - so the render-load
        // percentage is recorded BEFORE it, once per block,
        // from the actual `render_block` cost only.
        loop {
            match self.io.sample_tx.try_send(out) {
                Ok(()) => break,
                Err(mpsc::TrySendError::Full(returned)) => out = returned,
                Err(mpsc::TrySendError::Disconnected(_)) => return false,
            }
            if self.io.stop_flag.load(Ordering::Relaxed) || self.io.stop_rx.try_recv().is_ok() {
                return false;
            }
            std::thread::sleep(self.cfg.delay / 10);
        }

        // Sleep until the next cadence point (90% of realtime) to
        // play in real time - UNLESS the queue is below the
        // target cushion (about two blocks buffered): then render
        // back-to-back to refill it so peak blocks never starve
        // the audio callback.
        let now = Instant::now();
        let end = start + self.cfg.delay;
        let cushion = (self.cfg.block * self.cfg.channels * 2) as i64;
        if self.stats.samples.load(Ordering::SeqCst) >= cushion && end > now {
            std::thread::sleep(end - now);
        }
        true
    }
}

/// 启动渲染线程：与原代码同一 `thread::Builder`、同一线程名、同一
/// `map_err(SynthError::Io)`，仅把循环体搬到 [`RenderState`] 的方法里。
pub(super) fn spawn(args: RenderArgs) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("lumino-gpu-synth-render".into())
        .spawn(move || {
            let mut state = RenderState::new(args);
            state.run();
        })
}
