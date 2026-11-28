//! 渲染并输出样本哈希 / 电平 / 工作量指标（REF-LGS-013 基准脚手架）。
//!
//! 用法:
//!   hash_render <midi> <sf2> <frames> <block> [选项]
//!
//! 选项:
//!   --channels <N>    显式指定 `SynthConfig::midi_channels`（16..=256，16 的倍数）
//!   --auto-channels   按 MIDI 文件实际端口上界推导：`(max_port+1)*16`，钳到 16..=256
//!   --raw <out.f32>   落盘交错 f32（小端），用于逐样本对拍
//!   --wav <out.wav>   落盘 32-bit float WAV
//!   --json <out.json> 落盘机器可读指标（供 lgs_ab.py / CI 汇总）
//!   --prewarm         渲染前调用 prewarm_midi_file（覆盖实时预热路径）
//!   --no-effects      关闭音源滤波（REF-LGS-005 的未滤波验收路径）
//!   --sf-stats        只打印音色库采样峰值/越界统计后退出（REF-LGS-007）
//!   --load-twice      再加载一次音色库并打印 fresh/cached 耗时（REF-LGS-014）
//!
//! 哈希为 FNV-1a 64，覆盖所有样本的 `to_bits()` 小端字节，便于 golden 比对。
//! `voice_frames` = Σ(每块提交声部数 × 块帧数)，除以墙钟即"声部帧率"
//! （voice-frames/s），可跨块长/通道配置横向比较。

use lumino_gpu_synth::{GpuSynth, InterpolationMode, MidiFile, SynthConfig};
use std::fmt::Write as _;
use std::io::Write;
use std::time::{Duration, Instant};

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn write_wav_f32(
    path: &str,
    samples: &[f32],
    channels: u16,
    sample_rate: u32,
) -> std::io::Result<()> {
    let data_len = (samples.len() * 4) as u32;
    let byte_rate = sample_rate * u32::from(channels) * 4;
    let block_align = channels * 4;

    let mut f = std::fs::File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&3u16.to_le_bytes())?; // IEEE float
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&block_align.to_le_bytes())?;
    f.write_all(&32u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

/// 最小 JSON 字符串转义（路径里可能出现 `\` 与 `"`）。
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn usage() -> ! {
    eprintln!(
        "用法: hash_render <midi> <sf2> <frames> <block> \
         [--channels N | --auto-channels] [--raw out.f32] [--wav out.wav] \
         [--json out.json] [--prewarm] [--no-effects] [--max-voices N]"
    );
    std::process::exit(2);
}

/// 解析后的命令行参数；字符串借用自 `std::env::args()`。
struct Cli<'a> {
    midi: &'a str,
    sf2: &'a str,
    frames: u64,
    block: usize,
    raw_out: Option<&'a str>,
    wav_out: Option<&'a str>,
    json_out: Option<&'a str>,
    explicit_channels: Option<usize>,
    auto_channels: bool,
    prewarm: bool,
    no_effects: bool,
    max_voices: usize,
    sinc: bool,
    sf_stats: bool,
    load_twice: bool,
}

/// 样本侧报告口径：哈希、电平、墙钟与派生比率。
struct AudioMetrics {
    frames: u64,
    channels: u32,
    sample_rate: u32,
    wall: Duration,
    wall_secs: f64,
    audio_secs: f64,
    realtime: f64,
    samples_len: usize,
    hash: u64,
    peak: f64,
    rms: f64,
}

/// 引擎侧报告口径：通道配置、声部工作量与采样内存。
struct EngineMetrics {
    max_port: u8,
    midi_channels: usize,
    auto_channels: bool,
    no_effects: bool,
    max_voices: usize,
    voices_peak: u64,
    voice_frames: u64,
    voice_frames_per_s: f64,
    sample_gpu_bytes: u64,
    sample_cpu_bytes: u64,
    sample_resample_bytes: u64,
}

/// 解析位置参数与选项；解析顺序与错误文案保持历史口径。
fn parse_args(args: &[String]) -> Cli<'_> {
    if args.len() < 5 {
        usage();
    }
    let midi = &args[1];
    let sf2 = &args[2];
    let frames: u64 = args[3].parse().expect("frames 应为整数");
    let block: usize = args[4].parse().expect("block 应为整数");

    let mut cli = Cli {
        midi,
        sf2,
        frames,
        block,
        raw_out: None,
        wav_out: None,
        json_out: None,
        explicit_channels: None,
        auto_channels: false,
        prewarm: false,
        no_effects: false,
        max_voices: 0,
        sinc: false,
        sf_stats: false,
        load_twice: false,
    };
    let mut i = 5;
    while i < args.len() {
        match args[i].as_str() {
            "--raw" if i + 1 < args.len() => {
                cli.raw_out = Some(&args[i + 1]);
                i += 2;
            }
            "--wav" if i + 1 < args.len() => {
                cli.wav_out = Some(&args[i + 1]);
                i += 2;
            }
            "--json" if i + 1 < args.len() => {
                cli.json_out = Some(&args[i + 1]);
                i += 2;
            }
            "--channels" if i + 1 < args.len() => {
                cli.explicit_channels = Some(args[i + 1].parse().expect("--channels 应为整数"));
                i += 2;
            }
            "--auto-channels" => {
                cli.auto_channels = true;
                i += 1;
            }
            "--prewarm" => {
                cli.prewarm = true;
                i += 1;
            }
            "--no-effects" => {
                cli.no_effects = true;
                i += 1;
            }
            "--max-voices" if i + 1 < args.len() => {
                cli.max_voices = args[i + 1].parse().expect("--max-voices 应为整数");
                i += 2;
            }
            "--sinc" => {
                cli.sinc = true;
                i += 1;
            }
            "--sf-stats" => {
                cli.sf_stats = true;
                i += 1;
            }
            "--load-twice" => {
                cli.load_twice = true;
                i += 1;
            }
            other => {
                eprintln!("未知参数: {other}");
                usage();
            }
        }
    }
    if cli.auto_channels && cli.explicit_channels.is_some() {
        eprintln!("--auto-channels 与 --channels 不能同时使用");
        std::process::exit(2);
    }
    cli
}

/// `--sf-stats` 路径：只统计采样峰值/越界后退出。
fn print_sf_stats(sf2: &str, no_effects: bool, sample_rate: u32) {
    let sf = lumino_gpu_synth::SoundFont::load(sf2, 0, 0, !no_effects, sample_rate)
        .expect("load_soundfont");
    let mut max_abs = 0.0f32;
    let mut over = 0usize;
    let mut total = 0usize;
    let mut worst = (0usize, 0.0f32);
    for id in 0..sf.sample_count() {
        for &v in sf.sample_data(id).iter() {
            let a = v.abs();
            if a.is_finite() {
                total += 1;
                if a > 1.0 {
                    over += 1;
                }
                if a > max_abs {
                    max_abs = a;
                }
                if a > worst.1 {
                    worst = (id, a);
                }
            }
        }
    }
    println!(
        "sf_stats samples={} frames={} max_abs={max_abs:.6} worst_sample={} over_1={over} ({:.4}%)",
        sf.sample_count(),
        total,
        worst.0,
        over as f64 / total.max(1) as f64 * 100.0
    );
}

/// 组装 `SynthConfig`；字段赋值口径与历史版本逐行一致。
fn build_config(
    block: usize,
    max_voices: usize,
    midi_channels: usize,
    no_effects: bool,
    sinc: bool,
    sample_rate: u32,
) -> SynthConfig {
    SynthConfig {
        sample_rate,
        block_size: block,
        max_voices,
        max_voices_per_key: 0,
        midi_channels,
        show_progress: false,
        use_effects: !no_effects,
        interpolation: if sinc {
            InterpolationMode::Point64Sinc
        } else {
            InterpolationMode::Linear
        },
        ..SynthConfig::default()
    }
}

/// 解析端口上界与通道配置；解析时机保持历史口径（早于建引擎）。
fn prepare_config(cli: &Cli<'_>, sample_rate: u32) -> (SynthConfig, u8, usize) {
    // 端口上界只用于解析通道配置；渲染时引擎会再解析一次（解析成本不计入
    // 下面的 wall，口径与 5080 基线一致）。
    let max_port = MidiFile::load(cli.midi, sample_rate)
        .expect("MidiFile::load")
        .max_port;
    // 默认保持历史行为（16 = 单端口）；只有 --auto-channels 才按文件推导。
    let midi_channels = if cli.auto_channels {
        ((usize::from(max_port) + 1) * 16).clamp(16, 256)
    } else {
        cli.explicit_channels.unwrap_or(16)
    };
    let config = build_config(
        cli.block,
        cli.max_voices,
        midi_channels,
        cli.no_effects,
        cli.sinc,
        sample_rate,
    );
    (config, max_port, midi_channels)
}

/// 逐样本累积小端字节、峰值与平方和；累加顺序不得改变。
fn analyze_samples(samples: &[f32]) -> (Vec<u8>, f64, f64) {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    let mut peak = 0.0f64;
    let mut sq = 0.0f64;
    for &s in samples {
        bytes.extend_from_slice(&s.to_bits().to_le_bytes());
        let a = s.abs() as f64;
        if a > peak {
            peak = a;
        }
        sq += a * a;
    }
    let rms = (sq / samples.len().max(1) as f64).sqrt();
    (bytes, peak, rms)
}

/// 建引擎、加载音源，并按选项执行 reload / 预热路径。
fn prepare_synth(cli: &Cli<'_>, config: SynthConfig) -> GpuSynth {
    let mut synth = GpuSynth::new(config).expect("GpuSynth::new");
    let t_load = Instant::now();
    synth.load_soundfont(cli.sf2, 0, 0).expect("load_soundfont");
    let load_fresh = t_load.elapsed();
    if cli.load_twice {
        // REF-DATA-001：先预热把采样真正上传，再重载验证「换音源回收采样池」。
        synth
            .prewarm_midi_file(cli.midi)
            .expect("prewarm before reload");
        let (gpu_before, cpu_before, resample_before) = synth.sample_memory_breakdown();
        // REF-LGS-014：旧句柄仍在 → 第二次加载应命中进程级缓存（不重新解析）。
        let t_cached = Instant::now();
        synth
            .load_soundfont(cli.sf2, 0, 0)
            .expect("load_soundfont (cached)");
        let (gpu_after, cpu_after, resample_after) = synth.sample_memory_breakdown();
        println!(
            "load_fresh={:.1}ms load_cached={:.3}ms gpu_before_reload={} gpu_after_reload={} cpu_before_reload={} cpu_after_reload={} resample_before_reload={} resample_after_reload={}",
            load_fresh.as_secs_f64() * 1e3,
            t_cached.elapsed().as_secs_f64() * 1e3,
            gpu_before,
            gpu_after,
            cpu_before,
            cpu_after,
            resample_before,
            resample_after
        );
    }
    synth
}

/// 打印逐行指标；格式串与参数顺序不得改动（golden 输出口径）。
fn print_report(audio: &AudioMetrics, engine: &EngineMetrics) {
    println!(
        "frames={} channels={} sample_rate={}",
        audio.frames, audio.channels, audio.sample_rate
    );
    println!(
        "wall={:?} audio={:.3}s realtime={:.3}x",
        audio.wall, audio.audio_secs, audio.realtime
    );
    println!("samples={} fnv1a64={:016x}", audio.samples_len, audio.hash);
    println!("peak={:.9} rms={:.9}", audio.peak, audio.rms);
    println!(
        "midi_max_port={} midi_channels={} auto_channels={} no_effects={} max_voices={}",
        engine.max_port,
        engine.midi_channels,
        engine.auto_channels,
        engine.no_effects,
        engine.max_voices
    );
    println!(
        "voices_peak={} voice_frames={} voice_frames_per_s={:.0}",
        engine.voices_peak, engine.voice_frames, engine.voice_frames_per_s
    );
    println!(
        "sample_gpu_bytes={} sample_cpu_bytes={} sample_resample_bytes={}",
        engine.sample_gpu_bytes, engine.sample_cpu_bytes, engine.sample_resample_bytes
    );
}

/// 按选项落盘 raw / wav / json，并打印对应的 written 行。
fn write_outputs(
    cli: &Cli<'_>,
    samples: &[f32],
    bytes: &[u8],
    audio: &AudioMetrics,
    engine: &EngineMetrics,
) {
    if let Some(p) = cli.raw_out {
        std::fs::write(p, bytes).expect("write raw");
        println!("raw written: {p}");
    }
    if let Some(p) = cli.wav_out {
        write_wav_f32(p, samples, audio.channels as u16, audio.sample_rate).expect("write wav");
        println!("wav written: {p}");
    }
    if let Some(p) = cli.json_out {
        let no_limiter = std::env::var_os("LUMINO_NO_LIMITER").is_some();
        let json = format!(
            concat!(
                "{{\n",
                "  \"tool\": \"hash_render\",\n",
                "  \"version\": \"{}\",\n",
                "  \"midi\": \"{}\",\n",
                "  \"sf2\": \"{}\",\n",
                "  \"frames\": {},\n",
                "  \"block\": {},\n",
                "  \"sample_rate\": {},\n",
                "  \"midi_max_port\": {},\n",
                "  \"midi_channels\": {},\n",
                "  \"out_channels\": {},\n",
                "  \"auto_channels\": {},\n",
                "  \"no_effects\": {},\n",
                "  \"max_voices\": {},\n",
                "  \"audio_s\": {:.6},\n",
                "  \"wall_s\": {:.6},\n",
                "  \"realtime\": {:.6},\n",
                "  \"samples\": {},\n",
                "  \"fnv1a64\": \"{:016x}\",\n",
                "  \"peak\": {:.9},\n",
                "  \"rms\": {:.9},\n",
                "  \"voices_peak\": {},\n",
                "  \"voice_frames\": {},\n",
                "  \"voice_frames_per_s\": {:.1},\n",
                "  \"sample_gpu_bytes\": {},\n",
                "  \"sample_cpu_bytes\": {},\n",
                "  \"sample_resample_bytes\": {},\n",
                "  \"no_limiter\": {}\n",
                "}}\n"
            ),
            env!("CARGO_PKG_VERSION"),
            json_escape(cli.midi),
            json_escape(cli.sf2),
            cli.frames,
            cli.block,
            audio.sample_rate,
            engine.max_port,
            engine.midi_channels,
            audio.channels,
            cli.auto_channels,
            cli.no_effects,
            cli.max_voices,
            audio.audio_secs,
            audio.wall_secs,
            audio.realtime,
            audio.samples_len,
            audio.hash,
            audio.peak,
            audio.rms,
            engine.voices_peak,
            engine.voice_frames,
            engine.voice_frames_per_s,
            engine.sample_gpu_bytes,
            engine.sample_cpu_bytes,
            engine.sample_resample_bytes,
            no_limiter
        );
        std::fs::write(p, json).expect("write json");
        println!("json written: {p}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cli = parse_args(&args);
    let sample_rate = 48_000u32;

    if cli.sf_stats {
        print_sf_stats(cli.sf2, cli.no_effects, sample_rate);
        return;
    }

    let (config, max_port, midi_channels) = prepare_config(&cli, sample_rate);
    let mut synth = prepare_synth(&cli, config);

    if cli.prewarm {
        // 覆盖实时路径：预热会扫描事件并构建 voice 模板（通道门控的敏感点）。
        synth
            .prewarm_midi_file(cli.midi)
            .expect("prewarm_midi_file");
    }
    let t = Instant::now();
    let res = synth
        .render_midi_frames(cli.midi, cli.frames)
        .expect("render_midi_frames");
    let wall = t.elapsed();

    let (bytes, peak, rms) = analyze_samples(&res.samples);
    let audio_secs = res.frames as f64 / res.sample_rate as f64;
    let wall_secs = wall.as_secs_f64();
    let realtime = audio_secs / wall_secs;
    let hash = fnv1a64(&bytes);
    let voice_frames = synth.voice_frames();
    let voices_peak = synth.peak_voice_count();
    let voice_frames_per_s = voice_frames as f64 / wall_secs;
    let (sample_gpu_bytes, sample_cpu_bytes, sample_resample_bytes) =
        synth.sample_memory_breakdown();

    let audio = AudioMetrics {
        frames: res.frames,
        channels: res.channels,
        sample_rate: res.sample_rate,
        wall,
        wall_secs,
        audio_secs,
        realtime,
        samples_len: res.samples.len(),
        hash,
        peak,
        rms,
    };
    let engine = EngineMetrics {
        max_port,
        midi_channels,
        auto_channels: cli.auto_channels,
        no_effects: cli.no_effects,
        max_voices: cli.max_voices,
        voices_peak,
        voice_frames,
        voice_frames_per_s,
        sample_gpu_bytes,
        sample_cpu_bytes,
        sample_resample_bytes,
    };
    print_report(&audio, &engine);
    write_outputs(&cli, &res.samples, &bytes, &audio, &engine);
}
