//! 内置 UI 音效播放（一次性样本）
//!
//! 目前只有一个音效：「钢管落地」（关于页 logo 彩蛋落地时播放，见 UI-007）。
//!
//! # 设计约束
//!
//! - **随程序分发、不依赖外部文件**：音频以 `include_bytes!` 内嵌，运行期只解码内存字节；
//! - **绝不阻塞 UI 线程**：`play` / `prewarm` 只往专属工作线程发一条命令即返回；
//! - **音频回调零锁、零分配**：工作线程持有 cpal 流与 `Arc<Playback>`，回调仅读两个
//!   原子量并按游标搬运样本（沿用仓库「音频回调不持互斥量」的既定纪律）；
//! - **失败静默降级**：无输出设备 / 设备被独占 / 解码失败都只告警一次，不影响彩蛋其余流程。
//!
//! # 播放延迟
//!
//! 首次播放需「解码 + 解析设备 + 开流」（数十毫秒量级），若与落地同帧发起会有可听延迟，
//! 故调用方应在彩蛋**序列开始**时调 [`prewarm`]（只初始化不出声），落地瞬间再 [`play`]。
//!
//! # 采样率/声道适配
//!
//! 内嵌素材为 48 kHz 立体声，而设备默认输出可能是 44.1 kHz 或单声道，故播放前按设备
//! 实际参数做一次线性重采样 + 声道映射（见 [`adapt_to_device`]）。

use std::{
    io::Cursor,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio_devices::resolve_audio_output_device;

/// 内置「钢管落地」音效
///
/// 素材：`钢管 (1).mp3`（使用者提供，曲源标注 NCS）→ 仓库内 `resources/sounds/pipe-impact.mp3`
/// 规格：MP3 / 48 kHz / 立体声 / 2.366 s / 166 kbps / 49,059 B（峰值 0.0 dBFS、均值 −4.1 dB）
const PIPE_IMPACT_MP3: &[u8] = include_bytes!("../../../../resources/sounds/pipe-impact.mp3");

/// 播放增益。
///
/// 素材峰值实测 **0.0 dBFS**（满刻度）、均值 −4.1 dB，而彩蛋是**非预期触发**的隐藏交互，
/// 以满刻度直接回放容易惊吓用户（且下游任何增益都会削波）。这里留约 −4.4 dB 余量；
/// 如需完全忠实原文件，把该常量改为 `1.0` 即可（单点可调）。
const SFX_GAIN: f32 = 0.6;

/// 判定「前导静音」的能量阈值（约 −60 dBFS）
const LEAD_SILENCE_FLOOR: f32 = 0.001;

/// 前导静音最多裁掉多长（秒）
const LEAD_SILENCE_MAX: f32 = 0.2;

/// 前导静音裁剪时向前保留的帧数（≈0.08 ms，避免削掉软起振的头几个样本）
const LEAD_SILENCE_KEEP_FRAMES: usize = 4;

/// 播放结束后等待多久再暂停流（等最后一个缓冲块播完，避免尾部被切）
const TAIL_MARGIN: Duration = Duration::from_millis(150);

/// 解码结果（交错样本 + 原生参数）
#[derive(Debug, Clone)]
struct DecodedAudio {
    /// 交错（interleaved）样本
    samples: Vec<f32>,
    /// 声道数
    channels: usize,
    /// 采样率（Hz）
    sample_rate: u32,
}

/// 工作线程持有的播放状态（音频回调只读它）
///
/// 回调内**不得**出现锁：`playing` 与 `cursor` 都是原子量，`samples` 是不可变共享切片。
struct Playback {
    /// 已按设备参数适配的交错样本
    samples: Arc<Vec<f32>>,
    /// 读取游标（样本索引，非帧）
    cursor: AtomicUsize,
    /// 是否正在播放
    playing: AtomicBool,
    /// 起播代次：每次 `Engine::start` 自增。
    ///
    /// 回调只在「本批处理的起止代次一致」时才写回游标——否则一次重新起播
    /// （归零游标）会被在途回调的旧游标覆盖，表现为「第二次播放不从头上开始」。
    generation: AtomicU64,
}

/// 发给工作线程的命令
enum Command {
    /// 只初始化（解码 + 开流 + 暂停），不出声
    Prewarm(Option<String>),
    /// 初始化（若需要）并播放
    Play(Option<String>),
}

/// 工作线程句柄（命令发送端）
struct Worker {
    tx: mpsc::Sender<Command>,
}

impl Worker {
    /// 懒启动工作线程
    fn get_or_start() -> Option<&'static Worker> {
        static WORKER: OnceLock<Option<Worker>> = OnceLock::new();
        WORKER
            .get_or_init(|| {
                let (tx, rx) = mpsc::channel::<Command>();
                match std::thread::Builder::new()
                    .name("lumino-ui-sfx".to_string())
                    .spawn(move || worker_loop(rx))
                {
                    Ok(_) => Some(Worker { tx }),
                    Err(e) => {
                        tracing::warn!("UI 音效线程创建失败，音效将不可用: {e}");
                        None
                    }
                }
            })
            .as_ref()
    }

    /// 发送命令（线程已死或启动失败时静默忽略）
    fn send(&self, command: Command) {
        if self.tx.send(command).is_err() {
            tracing::warn!("UI 音效线程已退出，本次音效请求被丢弃");
        }
    }
}

/// 预初始化音效播放（解码 + 打开输出流并保持暂停），用于消除落地瞬间的开流延迟。
///
/// `output_device` 为配置中的音频输出设备名（`None`/空 = 系统默认设备），
/// 与合成器使用同一套设备解析逻辑（`audio_devices::resolve_audio_output_device`）。
pub fn prewarm(output_device: Option<&str>) {
    if let Some(worker) = Worker::get_or_start() {
        worker.send(Command::Prewarm(output_device.map(str::to_string)));
    }
}

/// 播放「钢管落地」音效（首次调用会自动完成初始化）。
///
/// 该函数**立即返回**，绝不阻塞调用线程；任何失败都只记日志。
pub fn play_pipe_impact(output_device: Option<&str>) {
    if let Some(worker) = Worker::get_or_start() {
        worker.send(Command::Play(output_device.map(str::to_string)));
    }
}

/// 工作线程每轮等待命令的最长时间（同时也是收尾检查的粒度）。
///
/// 20ms 与旧实现 `play()` 内的轮询间隔一致：足够让「播完 → 暂停流」的尾部
/// 处理不丢缓冲块，又不会让空闲线程空转。
const COMMAND_TICK: Duration = Duration::from_millis(20);

/// 播放收尾调度（纯状态机，便于单测）。
///
/// 语义（旧实现把两件事混在一起，导致 `TAIL_MARGIN` 形同虚设）：
/// - 素材播完（回调把 `playing` 翻 `false`）**之后**再等 [`TAIL_MARGIN`] 才暂停流
///   ——回调是在「把最后一个样本写进设备缓冲」时翻 false，此刻设备还没播出去，
///   立即 `pause()` 会在部分后端丢掉缓冲残余尾部；
/// - 同时保留硬上限（素材时长 + 尾音余量），兜住「回调异常未翻 `playing`」。
#[derive(Debug, Clone, Copy, Default)]
struct TailSchedule {
    /// 硬上限：到点必须收尾
    hard_deadline: Option<Instant>,
    /// 首次观察到停播后 + [`TAIL_MARGIN`] 的收尾时刻
    pause_after: Option<Instant>,
}

impl TailSchedule {
    /// 起播时重置调度。
    fn arm(&mut self, now: Instant, duration: Duration) {
        self.hard_deadline = Some(now + duration + TAIL_MARGIN);
        self.pause_after = None;
    }

    /// 是否处于「等待收尾」状态。
    fn is_armed(&self) -> bool {
        self.hard_deadline.is_some()
    }

    /// 推进一步：返回是否应当暂停输出流（返回 true 时自身复位为未武装）。
    fn poll(&mut self, now: Instant, playing: bool) -> bool {
        let Some(hard) = self.hard_deadline else {
            return false;
        };
        if self.pause_after.is_none() && !playing {
            // 首次观察到达「回调已送完样本」：再留尾音余量
            self.pause_after = Some(now + TAIL_MARGIN);
        }
        let due = now >= hard || self.pause_after.is_some_and(|d| now >= d);
        if due {
            self.hard_deadline = None;
            self.pause_after = None;
        }
        due
    }
}

/// 工作线程主循环：独占 cpal 流，处理命令并驱动播放收尾。
///
/// **不在命令处理里等待播放结束**：旧实现的 `play()` 会在循环内 `sleep` 至
/// 素材时长 + 尾音余量（≈2.5s），期间到达的 `Prewarm` / `Play` 全被推迟——
/// 彩蛋每进程只触发一次时看不出来，但一旦增加第二个 UI 音效就退化为
/// 「第二声延迟 2.5 秒」。现在改为 `recv_timeout` + 收尾状态机，播放期间的
/// 命令照常即时处理（新 `Play` 会立即重新起播）。
fn worker_loop(rx: mpsc::Receiver<Command>) {
    let mut engine: Option<Engine> = None;
    let mut init_failed = false;
    let mut schedule = TailSchedule::default();

    loop {
        let command = match rx.recv_timeout(COMMAND_TICK) {
            Ok(command) => Some(command),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            // 发送端已全部丢弃（进程退出或 Worker 被回收）：结束线程
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        if let Some(command) = command {
            let device = match &command {
                Command::Prewarm(d) | Command::Play(d) => d.clone(),
            };

            if engine.is_none() && !init_failed {
                match Engine::new(device.as_deref()) {
                    Ok(e) => engine = Some(e),
                    Err(e) => {
                        tracing::warn!("UI 音效初始化失败，本次运行内不再尝试: {e}");
                        init_failed = true;
                    }
                }
            }

            if let Command::Play(_) = &command
                && let Some(engine) = engine.as_mut()
            {
                let now = Instant::now();
                engine.start();
                schedule.arm(now, engine.duration);
            }
        }

        // 收尾检查：与命令处理解耦，故命令不会因等待播放结束而被推迟。
        if schedule.is_armed() {
            let playing = engine.as_ref().is_some_and(Engine::is_playing);
            if schedule.poll(Instant::now(), playing)
                && let Some(engine) = engine.as_mut()
            {
                engine.pause_stream();
            }
        }
    }
}

/// 音效输出引擎（cpal 流 + 播放状态）
struct Engine {
    /// 输出流（保持存活；播完暂停，避免常驻静音回调）
    stream: cpal::Stream,
    /// 与音频回调共享的播放状态
    state: Arc<Playback>,
    /// 素材时长（用于决定何时暂停流）
    duration: Duration,
}

impl Engine {
    /// 解码内嵌素材、解析设备、按设备参数适配并开流（初始为暂停）
    fn new(device_name: Option<&str>) -> Result<Self, String> {
        let decoded = decode_pipe_impact()?;

        let device = resolve_audio_output_device(device_name)
            .or_else(|| cpal::default_host().default_output_device())
            .ok_or_else(|| "无可用音频输出设备".to_string())?;

        let supported = device
            .default_output_config()
            .map_err(|e| format!("读取默认输出配置失败: {e}"))?;
        let channels = supported.channels() as usize;
        let sample_rate = supported.sample_rate().0;
        let config = supported.config();

        let adapted = adapt_to_device(&decoded, channels, sample_rate);
        let frames = adapted.len() / channels.max(1);
        let duration = Duration::from_secs_f32(frames as f32 / sample_rate.max(1) as f32);

        let state = Arc::new(Playback {
            samples: Arc::new(adapted),
            cursor: AtomicUsize::new(0),
            playing: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        });

        let callback_state = Arc::clone(&state);
        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // 回调内零锁零分配：仅原子读写 + 样本搬运
                    if !callback_state.playing.load(Ordering::Acquire) {
                        output.fill(0.0);
                        return;
                    }
                    let samples = &callback_state.samples;
                    // 起播代次：若本批处理期间发生「重新起播」，则作废本批的游标写回，
                    // 否则会把归零后的游标覆盖成旧值（第二次播放不从头开始）。
                    let generation = callback_state.generation.load(Ordering::Acquire);
                    let mut cursor = callback_state.cursor.load(Ordering::Relaxed);
                    for slot in output.iter_mut() {
                        if cursor < samples.len() {
                            *slot = samples[cursor];
                            cursor += 1;
                        } else {
                            *slot = 0.0;
                        }
                    }
                    if callback_state.generation.load(Ordering::Acquire) != generation {
                        return; // 期间已重新起播：本批游标写回作废
                    }
                    callback_state.cursor.store(cursor, Ordering::Relaxed);
                    if cursor >= samples.len() {
                        callback_state.playing.store(false, Ordering::Release);
                    }
                },
                |e| tracing::warn!("UI 音效输出流错误: {e}"),
                None,
            )
            .map_err(|e| format!("创建输出流失败: {e}"))?;

        // 初始暂停：仅在真正播放时开闸，避免常驻静音回调
        stream.pause().map_err(|e| format!("暂停输出流失败: {e}"))?;

        tracing::debug!(
            "UI 音效就绪：{} Hz / {} 声道 / {:.2}s",
            sample_rate,
            channels,
            duration.as_secs_f32()
        );

        Ok(Self {
            stream,
            state,
            duration,
        })
    }

    /// 起播（**非阻塞**）：关闸 → 归零游标 + 递增代次 → 开闸。
    ///
    /// 顺序有意为之：先关闸并递增 `generation`，在途音频回调即使已通过入口判定，
    /// 也会因代次变化而**放弃写回游标**，不会把归零后的游标覆盖成旧值
    /// （旧实现会在第二次播放时表现为「不从头开始」）。
    /// 收尾（暂停流）由 [`TailSchedule`] 在命令循环里驱动，本函数不再阻塞。
    fn start(&mut self) {
        self.state.playing.store(false, Ordering::Release);
        self.state.cursor.store(0, Ordering::Relaxed);
        self.state.generation.fetch_add(1, Ordering::AcqRel);
        if let Err(e) = self.stream.play() {
            tracing::warn!("UI 音效起播失败: {e}");
            return;
        }
        self.state.playing.store(true, Ordering::Release);
    }

    /// 回调是否仍在送样本（收尾判据之一）。
    fn is_playing(&self) -> bool {
        self.state.playing.load(Ordering::Relaxed)
    }

    /// 暂停输出流：播完收尾，避免常驻静音回调。
    fn pause_stream(&mut self) {
        if let Err(e) = self.stream.pause() {
            tracing::debug!("UI 音效暂停失败（无碍）: {e}");
        }
    }
}

/// 解码内嵌的「钢管落地」素材（首次调用解码，之后由 `OnceLock` 复用）
fn decode_pipe_impact() -> Result<DecodedAudio, String> {
    static DECODED: OnceLock<Result<DecodedAudio, String>> = OnceLock::new();
    DECODED
        .get_or_init(|| decode_mp3_bytes(PIPE_IMPACT_MP3).map(trim_leading_silence))
        .clone()
}

/// 裁掉开头的前导数字静音。
///
/// **为什么必须做**：「落地即响」要求冲击瞬态与动画落地同帧，而 symphonia 解出的
/// 前导近静音是**编码器延迟**：MP3 编码器会在流头写入延迟样本，规范做法是解码器读
/// LAME 头后跳过；**ffmpeg 会跳、symphonia 不跳**（同素材对比，差值 ≈ LAME 典型
/// 1105 样本 ≈ 23 ms）。当前素材实测前导约 23 ms（素材自身无 preroll），未裁剪时
/// 起振会迟到约 47 ms（含素材自身 ~24 ms 起振爬升）——已由断言的**反证**确认可测。
///
/// 裁剪按能量阈值判定，与素材本身解耦（换素材同样适用）；只裁开头、不裁结尾
/// （尾音是素材的自然衰减，静音尾巴无害）。
fn trim_leading_silence(mut audio: DecodedAudio) -> DecodedAudio {
    let channels = audio.channels.max(1);
    let frames = audio.samples.len() / channels;
    if frames == 0 {
        return audio;
    }

    let max_scan = ((audio.sample_rate as f32 * LEAD_SILENCE_MAX) as usize).min(frames);
    let mut lead = 0usize;
    while lead < max_scan {
        let peak = (0..channels).fold(0.0_f32, |acc, ch| {
            acc.max(audio.samples[lead * channels + ch].abs())
        });
        if peak > LEAD_SILENCE_FLOOR {
            break;
        }
        lead += 1;
    }

    // 全都低于阈值（异常素材）则不动，避免把整段样本裁空
    if lead == 0 || lead >= frames {
        return audio;
    }

    let cut = lead.saturating_sub(LEAD_SILENCE_KEEP_FRAMES);
    if cut > 0 {
        audio.samples.drain(..cut * channels);
        tracing::debug!(
            "UI 音效：裁掉前导静音 {} 帧（{:.1} ms）",
            cut,
            cut as f32 / audio.sample_rate.max(1) as f32 * 1000.0
        );
    }
    audio
}

/// 解码内存中的 MP3 字节为交错 f32 样本
///
/// 与 `lumino-gpu-synth` 的 SFZ 采样解码同源（同为 symphonia），差别只在数据来源为
/// 内存字节、输出为交错布局（cpal 的回调缓冲即交错布局）。
///
/// 形参取 `&'static [u8]`：`MediaSourceStream` 需要 `Box<dyn MediaSource + 'static>`，
/// 内嵌素材天然满足，故直接零拷贝借用，不做一次 100 KB 级的内存副本。
fn decode_mp3_bytes(bytes: &'static [u8]) -> Result<DecodedAudio, String> {
    use symphonia::core::{
        audio::{AudioBufferRef, Signal},
        codecs::DecoderOptions,
        conv::IntoSample,
        formats::FormatOptions,
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    };

    let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("探测音频格式失败: {e:?}"))?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| "音频文件无可用轨道".to_string())?;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(48_000);
    let track_id = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("创建解码器失败: {e:?}"))?;

    let mut channels: Vec<Vec<f32>> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(format!("读取音频包失败: {e:?}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder
            .decode(&packet)
            .map_err(|e| format!("解码音频包失败: {e:?}"))?;

        if channels.is_empty() {
            channels = vec![Vec::new(); decoded.spec().channels.count()];
        }

        macro_rules! append {
            ($buf:expr) => {{
                // 声道数以首个解码缓冲为准；后续缓冲声道数异常时只拷贝交集，避免越界
                let available = $buf.spec().channels.count();
                for (ci, samples) in channels.iter_mut().enumerate() {
                    if ci < available {
                        samples.extend(
                            $buf.chan(ci)
                                .iter()
                                .map(|s| IntoSample::<f32>::into_sample(*s)),
                        );
                    }
                }
            }};
        }
        match decoded {
            AudioBufferRef::U8(b) => append!(b),
            AudioBufferRef::U16(b) => append!(b),
            AudioBufferRef::U24(b) => append!(b),
            AudioBufferRef::U32(b) => append!(b),
            AudioBufferRef::S8(b) => append!(b),
            AudioBufferRef::S16(b) => append!(b),
            AudioBufferRef::S24(b) => append!(b),
            AudioBufferRef::S32(b) => append!(b),
            AudioBufferRef::F32(b) => append!(b),
            AudioBufferRef::F64(b) => append!(b),
        }
    }

    if channels.is_empty() || channels[0].is_empty() {
        return Err("音频解码结果为空".to_string());
    }
    let channel_count = channels.len();

    // 逐通道 → 交错，并施加播放增益
    let frames = channels[0].len();
    let mut samples = Vec::with_capacity(frames * channel_count);
    for frame in 0..frames {
        for channel in &channels {
            // 各通道帧数理论上一致；异常时以静音补齐，避免越界
            let value = channel.get(frame).copied().unwrap_or(0.0);
            samples.push((value * SFX_GAIN).clamp(-1.0, 1.0));
        }
    }

    Ok(DecodedAudio {
        samples,
        channels: channel_count,
        sample_rate,
    })
}

/// 把解码结果适配到设备参数（线性重采样 + 声道映射）
///
/// - 采样率一致：不重采样；
/// - 声道一致：直通；单声道源 → 多声道目标：复制；立体声 → 单声道：取左右均值。
fn adapt_to_device(decoded: &DecodedAudio, out_channels: usize, out_rate: u32) -> Vec<f32> {
    let src_channels = decoded.channels.max(1);
    let out_channels = out_channels.max(1);
    let src_rate = decoded.sample_rate.max(1);
    let out_rate = out_rate.max(1);
    let src_frames = decoded.samples.len() / src_channels;
    if src_frames == 0 {
        return Vec::new();
    }

    let ratio = src_rate as f64 / out_rate as f64;
    let out_frames = ((src_frames as f64) / ratio).ceil().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_frames * out_channels);

    for frame in 0..out_frames {
        let pos = frame as f64 * ratio;
        let i0 = (pos.floor() as usize).min(src_frames - 1);
        let i1 = (i0 + 1).min(src_frames - 1);
        let frac = (pos - pos.floor()) as f32;

        for channel in 0..out_channels {
            let value = match (src_channels, out_channels) {
                (2, 1) => {
                    let l = read_frame(decoded, i0, 0);
                    let r = read_frame(decoded, i0, 1);
                    let l2 = read_frame(decoded, i1, 0);
                    let r2 = read_frame(decoded, i1, 1);
                    let a = (l + r) * 0.5;
                    let b = (l2 + r2) * 0.5;
                    a + (b - a) * frac
                }
                (1, n) if n > 1 => {
                    let a = read_frame(decoded, i0, 0);
                    let b = read_frame(decoded, i1, 0);
                    a + (b - a) * frac
                }
                _ => {
                    // 声道直通；目标多于源时复用最后一个源声道
                    let sc = channel.min(src_channels - 1);
                    let a = read_frame(decoded, i0, sc);
                    let b = read_frame(decoded, i1, sc);
                    a + (b - a) * frac
                }
            };
            out.push(value.clamp(-1.0, 1.0));
        }
    }

    out
}

/// 读取交错缓冲中第 `frame` 帧的第 `channel` 个样本（越界返回 0）
fn read_frame(decoded: &DecodedAudio, frame: usize, channel: usize) -> f32 {
    decoded
        .samples
        .get(frame * decoded.channels.max(1) + channel)
        .copied()
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 「有效电平」判定线：峰值的 90%
    const ONSET_REACH_RATIO: f32 = 0.9;
    /// 容许的起振时间上限（毫秒）：超过则落地后听得出延迟
    const ONSET_MAX_MS: f32 = 30.0;

    // ── 播放收尾调度（S-M2/M3 回归）────────────────────────────────
    // 这些用例不碰音频设备：`TailSchedule` 是纯状态机，时刻由调用方注入。

    /// **尾音余量必须落在「回调送完样本」之后**——旧实现在 `playing` 翻 false
    /// 的当刻就 `pause()`，等于没等缓冲块播出去（部分后端会切掉尾部）。
    #[test]
    fn tail_schedule_waits_tail_margin_after_playback_ends() {
        let t0 = Instant::now();
        let duration = Duration::from_secs(2);
        let mut s = TailSchedule::default();
        s.arm(t0, duration);

        // 仍在送样本：不得收尾
        assert!(!s.poll(t0 + Duration::from_millis(1_900), true));
        // 回调刚送完（playing = false）的当刻：**不得**收尾
        let flipped = t0 + duration;
        assert!(
            !s.poll(flipped, false),
            "翻 false 当刻不得暂停流：此时缓冲块还没播出去（旧实现的行为）"
        );
        // 尾音余量未满：仍不得收尾
        assert!(!s.poll(flipped + TAIL_MARGIN - Duration::from_millis(1), false));
        // 到期收尾，且只收一次
        assert!(s.poll(flipped + TAIL_MARGIN, false), "尾音余量满后应收尾");
        assert!(!s.is_armed(), "收尾后必须复位");
        assert!(
            !s.poll(flipped + TAIL_MARGIN + Duration::from_secs(5), false),
            "未重新起播时不得再次暂停"
        );
    }

    /// 回调异常（`playing` 永不翻 false）时，硬上限必须兜住而不会永久挂着。
    #[test]
    fn tail_schedule_hard_deadline_fires_even_if_playing_never_clears() {
        let t0 = Instant::now();
        let duration = Duration::from_secs(2);
        let mut s = TailSchedule::default();
        s.arm(t0, duration);

        assert!(!s.poll(t0 + duration, true), "硬上限未到且仍在播放：不收尾");
        assert!(
            s.poll(t0 + duration + TAIL_MARGIN, true),
            "硬上限到点必须收尾（否则流永不暂停、常驻静音回调）"
        );
        assert!(!s.is_armed());
    }

    /// **命令循环不被阻塞的直接证据**：起播期间可以立即重新武装（= 处理新的 Play），
    /// 且旧收尾时刻不得继续生效。
    #[test]
    fn tail_schedule_rearm_replaces_previous_deadline() {
        let t0 = Instant::now();
        let mut s = TailSchedule::default();
        s.arm(t0, Duration::from_secs(2));
        assert!(!s.poll(t0 + Duration::from_secs(2), false), "开始等尾音");

        // 第二次起播（旧实现这里会被上一次播放阻塞 ≈2.5s 才轮到）
        let t1 = t0 + Duration::from_secs(2) + Duration::from_millis(10);
        s.arm(t1, Duration::from_secs(3));

        assert!(
            !s.poll(t1 + Duration::from_millis(100), true),
            "重新起播后不得沿用上一次的收尾时刻"
        );
        assert!(s.poll(t1 + Duration::from_secs(3) + TAIL_MARGIN, false));
    }

    /// 未起播时收尾检查必须完全惰性（否则空闲线程会反复 pause 已暂停的流）。
    #[test]
    fn tail_schedule_poll_is_inert_before_arm() {
        let mut s = TailSchedule::default();
        assert!(!s.is_armed());
        assert!(!s.poll(Instant::now(), false), "未起播时不得触发暂停流");
    }

    #[test]
    fn test_trim_leading_silence_removes_encoder_delay() {
        // 100 帧数字静音 + 4 帧冲击
        let mut samples = vec![0.0_f32; 100 * 2];
        samples.extend([0.9_f32, -0.9, 0.5, -0.5, 0.3, -0.3, 0.2, -0.2]);
        let audio = DecodedAudio {
            samples,
            channels: 2,
            sample_rate: 48_000,
        };

        let trimmed = trim_leading_silence(audio);
        // 保留 LEAD_SILENCE_KEEP_FRAMES 帧前导静音，其后紧接冲击样本
        assert_eq!(
            trimmed.samples.len(),
            LEAD_SILENCE_KEEP_FRAMES * 2 + 8,
            "应裁掉前导静音、仅保留少量 preroll 与全部冲击样本"
        );
        // preroll 仍是静音，越过 preroll 的第一帧即冲击
        let onset = LEAD_SILENCE_KEEP_FRAMES * 2;
        assert!(
            (trimmed.samples[onset].abs() - 0.9).abs() < f32::EPSILON,
            "preroll 之后首帧应为冲击样本，实际 {}",
            trimmed.samples[onset]
        );
    }

    #[test]
    fn test_trim_leading_silence_keeps_silent_input_untouched() {
        let audio = DecodedAudio {
            samples: vec![0.0; 200],
            channels: 2,
            sample_rate: 48_000,
        };
        let trimmed = trim_leading_silence(audio);
        assert_eq!(trimmed.samples.len(), 200, "全静音素材不应被裁空");
    }

    /// 前提特征化：内嵌素材在 symphonia 侧确实带前导近静音，故 [`trim_leading_silence`] 必要。
    ///
    /// 当前素材（`钢管 (1).mp3`，48 kHz / 2.366 s）自身**无**前导静音（ffmpeg 侧实测为 0），
    /// 因此这里测到的前导几乎就是 symphonia **未跳过的 MP3 编码器延迟**（LAME 典型 1105 样本
    /// ≈ 23 ms）。断言给出 10~100 ms 的宽带：素材自身若带 preroll 会叠加进来，仍应落在带内。
    ///
    /// 该测试锁住「为什么必须有裁剪」这一前提——若将来 symphonia 开始按 LAME 头跳延迟，
    /// 前导会塌到 0，本测试会失败并提示裁剪逻辑已退化为空转（届时可复核是否仍需裁剪）。
    #[test]
    fn test_embedded_asset_carries_mp3_encoder_delay() {
        let raw = decode_mp3_bytes(PIPE_IMPACT_MP3).expect("内嵌素材应能解码");
        let channels = raw.channels;
        let lead_frames = raw
            .samples
            .chunks(channels)
            .take_while(|frame| frame.iter().all(|s| s.abs() <= LEAD_SILENCE_FLOOR))
            .count();
        let lead_ms = lead_frames as f32 / raw.sample_rate as f32 * 1000.0;
        assert!(
            lead_frames > 0,
            "symphonia 未跳编码器延迟：应测到前导近静音，实际 0"
        );
        assert!(
            (10.0..=100.0).contains(&lead_ms),
            "前导应为编码器延迟(±素材 preroll)，实际 {lead_ms:.1}ms"
        );
    }

    /// 管线行为断言（**刻意不钉死素材规格**，换素材不应改测试）。
    ///
    /// 素材的精确规格（体积/时长/峰值/哈希）记录在文档与 `PIPE_IMPACT_MP3` 的注释中；
    /// 本测试只保证「解得出、规格合理、落地即响、增益生效」这四件与管线有关的事。
    #[test]
    fn test_embedded_impact_decodes_with_immediate_onset() {
        let decoded = decode_pipe_impact().expect("内嵌「钢管落地」素材应能解码");

        assert_eq!(decoded.sample_rate, 48_000, "素材应为 48 kHz");
        assert_eq!(decoded.channels, 2, "素材应为立体声");

        let frames = decoded.samples.len() / decoded.channels;
        let seconds = frames as f32 / decoded.sample_rate as f32;
        assert!(
            (0.3..=10.0).contains(&seconds),
            "音效时长应在合理区间（十进制秒级），实际 {seconds}s"
        );

        // 「落地即响」的判据：必须在极短时间内达到有效电平，否则落地后会有可听的"空气"。
        //
        // 用「达到峰值 90% 的时间」而非「前 10ms 的绝对能量」：实录冲击本身常带 ~20ms
        // 起振爬升（当前素材峰值落在 25.3ms），10ms 窗口对实录过严；而 30ms 上限依然能
        // 抓住「编码器延迟未裁剪」这类真实缺陷（旧素材未裁剪时会迟到 ~60ms）。
        let peak = decoded.samples.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
        let threshold = peak * ONSET_REACH_RATIO;
        let onset_samples = decoded
            .samples
            .iter()
            .position(|s| s.abs() >= threshold)
            .expect("素材应存在达到峰值 90% 的样本");
        let onset_ms =
            onset_samples as f32 / decoded.channels as f32 / decoded.sample_rate as f32 * 1000.0;
        assert!(
            onset_ms <= ONSET_MAX_MS,
            "落地即响：应在 {ONSET_MAX_MS}ms 内达到有效电平（峰值 {peak:.3}），实际 {onset_ms:.1}ms"
        );
        assert!(
            peak <= SFX_GAIN + 0.02,
            "增益/钳制未生效：峰值 {peak} 超过 SFX_GAIN({SFX_GAIN})"
        );
        assert!(
            peak > 0.2,
            "素材应有真实信号（峰值应接近 SFX_GAIN），实际 {peak}"
        );
    }

    #[test]
    fn test_adapt_identity_keeps_samples() {
        let decoded = DecodedAudio {
            samples: vec![0.1, -0.1, 0.2, -0.2],
            channels: 2,
            sample_rate: 48_000,
        };
        let out = adapt_to_device(&decoded, 2, 48_000);
        assert_eq!(out, decoded.samples, "参数一致时应逐样本直通");
    }

    #[test]
    fn test_adapt_stereo_to_mono_downmixes() {
        let decoded = DecodedAudio {
            samples: vec![1.0, 0.0, 0.5, 0.5],
            channels: 2,
            sample_rate: 48_000,
        };
        let out = adapt_to_device(&decoded, 1, 48_000);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.5).abs() < 1e-6, "应取左右均值: {}", out[0]);
        assert!((out[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_adapt_mono_to_stereo_duplicates() {
        let decoded = DecodedAudio {
            samples: vec![0.3, 0.7],
            channels: 1,
            sample_rate: 48_000,
        };
        let out = adapt_to_device(&decoded, 2, 48_000);
        assert_eq!(out, vec![0.3, 0.3, 0.7, 0.7], "单声道应复制到双声道");
    }

    #[test]
    fn test_adapt_resamples_rate() {
        let frames = 480; // 48 kHz 下 10ms
        let decoded = DecodedAudio {
            samples: vec![0.0; frames],
            channels: 1,
            sample_rate: 48_000,
        };
        let out = adapt_to_device(&decoded, 1, 44_100);
        let expected = (frames as f64 * 44_100.0 / 48_000.0).ceil() as usize;
        assert!(
            out.len().abs_diff(expected) <= 1,
            "44.1kHz 目标帧数应约 {expected}，实际 {}",
            out.len()
        );
    }

    #[test]
    fn test_adapt_empty_source_is_safe() {
        let decoded = DecodedAudio {
            samples: Vec::new(),
            channels: 2,
            sample_rate: 48_000,
        };
        assert!(adapt_to_device(&decoded, 2, 48_000).is_empty());
    }
}
