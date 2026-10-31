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
//!
//! 哈希为 FNV-1a 64，覆盖所有样本的 `to_bits()` 小端字节，便于 golden 比对。
//! `voice_frames` = Σ(每块提交声部数 × 块帧数)，除以墙钟即"声部帧率"
//! （voice-frames/s），可跨块长/通道配置横向比较。

use lumino_gpu_synth::{GpuSynth, InterpolationMode, MidiFile, SynthConfig};
use std::fmt::Write as _;
use std::io::Write;
use std::time::Instant;

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        usage();
    }

    let midi = &args[1];
    let sf2 = &args[2];
    let frames: u64 = args[3].parse().expect("frames 应为整数");
    let block: usize = args[4].parse().expect("block 应为整数");
    let sample_rate = 48_000u32;

    let mut raw_out: Option<&str> = None;
    let mut wav_out: Option<&str> = None;
    let mut json_out: Option<&str> = None;
    let mut explicit_channels: Option<usize> = None;
    let mut auto_channels = false;
    let mut prewarm = false;
    let mut no_effects = false;
    let mut max_voices: usize = 0;
    let mut sinc = false;
    let mut sf_stats = false;
    let mut i = 5;
    while i < args.len() {
        match args[i].as_str() {
            "--raw" if i + 1 < args.len() => {
                raw_out = Some(&args[i + 1]);
                i += 2;
            }
            "--wav" if i + 1 < args.len() => {
                wav_out = Some(&args[i + 1]);
                i += 2;
            }
            "--json" if i + 1 < args.len() => {
                json_out = Some(&args[i + 1]);
                i += 2;
            }
            "--channels" if i + 1 < args.len() => {
                explicit_channels = Some(args[i + 1].parse().expect("--channels 应为整数"));
                i += 2;
            }
            "--auto-channels" => {
                auto_channels = true;
                i += 1;
            }
            "--prewarm" => {
                prewarm = true;
                i += 1;
            }
            "--no-effects" => {
                no_effects = true;
                i += 1;
            }
            "--max-voices" if i + 1 < args.len() => {
                max_voices = args[i + 1].parse().expect("--max-voices 应为整数");
                i += 2;
            }
            "--sinc" => {
                sinc = true;
                i += 1;
            }
            "--sf-stats" => {
                sf_stats = true;
                i += 1;
            }
            other => {
                eprintln!("未知参数: {other}");
                usage();
            }
        }
    }
    if auto_channels && explicit_channels.is_some() {
        eprintln!("--auto-channels 与 --channels 不能同时使用");
        std::process::exit(2);
    }

    if sf_stats {
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
        return;
    }

    // 端口上界只用于解析通道配置；渲染时引擎会再解析一次（解析成本不计入
    // 下面的 wall，口径与 5080 基线一致）。
    let max_port = MidiFile::load(midi, sample_rate)
        .expect("MidiFile::load")
        .max_port;
    // 默认保持历史行为（16 = 单端口）；只有 --auto-channels 才按文件推导。
    let midi_channels = if auto_channels {
        ((usize::from(max_port) + 1) * 16).clamp(16, 256)
    } else {
        explicit_channels.unwrap_or(16)
    };

    let config = SynthConfig {
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
    };

    let mut synth = GpuSynth::new(config).expect("GpuSynth::new");
    synth.load_soundfont(sf2, 0, 0).expect("load_soundfont");
    if prewarm {
        // 覆盖实时路径：预热会扫描事件并构建 voice 模板（通道门控的敏感点）。
        synth.prewarm_midi_file(midi).expect("prewarm_midi_file");
    }
    let t = Instant::now();
    let res = synth
        .render_midi_frames(midi, frames)
        .expect("render_midi_frames");
    let wall = t.elapsed();

    let mut bytes = Vec::with_capacity(res.samples.len() * 4);
    let mut peak = 0.0f64;
    let mut sq = 0.0f64;
    for &s in &res.samples {
        bytes.extend_from_slice(&s.to_bits().to_le_bytes());
        let a = s.abs() as f64;
        if a > peak {
            peak = a;
        }
        sq += a * a;
    }
    let rms = (sq / res.samples.len().max(1) as f64).sqrt();
    let audio_secs = res.frames as f64 / res.sample_rate as f64;
    let wall_secs = wall.as_secs_f64();
    let realtime = audio_secs / wall_secs;
    let hash = fnv1a64(&bytes);
    let voice_frames = synth.voice_frames();
    let voices_peak = synth.peak_voice_count();
    let voice_frames_per_s = voice_frames as f64 / wall_secs;
    let (sample_gpu_bytes, sample_cpu_bytes) = synth.sample_memory_bytes();

    println!(
        "frames={} channels={} sample_rate={}",
        res.frames, res.channels, res.sample_rate
    );
    println!(
        "wall={:?} audio={:.3}s realtime={:.3}x",
        wall, audio_secs, realtime
    );
    println!("samples={} fnv1a64={:016x}", res.samples.len(), hash);
    println!("peak={:.9} rms={:.9}", peak, rms);
    println!(
        "midi_max_port={} midi_channels={} auto_channels={} no_effects={} max_voices={}",
        max_port, midi_channels, auto_channels, no_effects, max_voices
    );
    println!(
        "voices_peak={} voice_frames={} voice_frames_per_s={:.0}",
        voices_peak, voice_frames, voice_frames_per_s
    );
    println!(
        "sample_gpu_bytes={} sample_cpu_bytes={}",
        sample_gpu_bytes, sample_cpu_bytes
    );

    if let Some(p) = raw_out {
        std::fs::write(p, &bytes).expect("write raw");
        println!("raw written: {p}");
    }
    if let Some(p) = wav_out {
        write_wav_f32(p, &res.samples, res.channels as u16, res.sample_rate).expect("write wav");
        println!("wav written: {p}");
    }
    if let Some(p) = json_out {
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
                "  \"no_limiter\": {}\n",
                "}}\n"
            ),
            env!("CARGO_PKG_VERSION"),
            json_escape(midi),
            json_escape(sf2),
            frames,
            block,
            res.sample_rate,
            max_port,
            midi_channels,
            res.channels,
            auto_channels,
            no_effects,
            max_voices,
            audio_secs,
            wall_secs,
            realtime,
            res.samples.len(),
            hash,
            peak,
            rms,
            voices_peak,
            voice_frames,
            voice_frames_per_s,
            sample_gpu_bytes,
            sample_cpu_bytes,
            no_limiter
        );
        std::fs::write(p, json).expect("write json");
        println!("json written: {p}");
    }
}
