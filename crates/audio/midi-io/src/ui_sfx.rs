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
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
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

/// 工作线程主循环：独占 cpal 流，串行处理命令
fn worker_loop(rx: mpsc::Receiver<Command>) {
    let mut engine: Option<Engine> = None;
    let mut init_failed = false;

    while let Ok(command) = rx.recv() {
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

        if let (Some(engine), Command::Play(_)) = (engine.as_mut(), command) {
            engine.play();
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
        });

        let callback_state = Arc::clone(&state);
        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // 回调内零锁零分配：仅原子读写 + 样本搬运
                    if !callback_state.playing.load(Ordering::Relaxed) {
                        output.fill(0.0);
                        return;
                    }
                    let samples = &callback_state.samples;
                    let mut cursor = callback_state.cursor.load(Ordering::Relaxed);
                    for slot in output.iter_mut() {
                        if cursor < samples.len() {
                            *slot = samples[cursor];
                            cursor += 1;
                        } else {
                            *slot = 0.0;
                        }
                    }
                    callback_state.cursor.store(cursor, Ordering::Relaxed);
                    if cursor >= samples.len() {
                        callback_state.playing.store(false, Ordering::Relaxed);
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

    /// 从头播放一次，并在播完后暂停流
    fn play(&mut self) {
        self.state.cursor.store(0, Ordering::Relaxed);
        self.state.playing.store(true, Ordering::Release);
        if let Err(e) = self.stream.play() {
            tracing::warn!("UI 音效起播失败: {e}");
            self.state.playing.store(false, Ordering::Relaxed);
            return;
        }

        // 等待播放结束再暂停。此处阻塞的是专属工作线程（不承担其它职责），
        // 且彩蛋序列每次运行最多触发一次，不存在命令堆积。
        let wait = self.duration + TAIL_MARGIN;
        let deadline = std::time::Instant::now() + wait;
        while std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            if !self.state.playing.load(Ordering::Relaxed) {
                break;
            }
        }
        // 期间若又收到播放请求（playing 被重新置位），不要暂停
        if !self.state.playing.load(Ordering::Relaxed)
            && let Err(e) = self.stream.pause()
        {
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
