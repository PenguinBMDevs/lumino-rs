//! 渲染并输出样本哈希 / 电平指标（REF-LGS-013 基准脚手架）。
//!
//! 用法:
//!   hash_render <midi> <sf2> <frames> <block> [--raw out.f32] [--wav out.wav]
//!
//! `--raw` 落盘交错 f32（小端），用于逐样本对拍；`--wav` 落盘 32-bit float WAV。
//! 哈希为 FNV-1a 64，覆盖所有样本的 `to_bits()` 小端字节，便于 golden 比对。

use lumino_gpu_synth::{GpuSynth, SynthConfig};
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!(
            "用法: hash_render <midi> <sf2> <frames> <block> [--raw out.f32] [--wav out.wav]"
        );
        std::process::exit(2);
    }

    let midi = &args[1];
    let sf2 = &args[2];
    let frames: u64 = args[3].parse().expect("frames 应为整数");
    let block: usize = args[4].parse().expect("block 应为整数");

    let mut raw_out: Option<&str> = None;
    let mut wav_out: Option<&str> = None;
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
            other => {
                eprintln!("未知参数: {other}");
                std::process::exit(2);
            }
        }
    }

    let config = SynthConfig {
        sample_rate: 48_000,
        block_size: block,
        max_voices: 0,
        max_voices_per_key: 0,
        show_progress: false,
        ..SynthConfig::default()
    };

    let mut synth = GpuSynth::new(config).expect("GpuSynth::new");
    synth.load_soundfont(sf2, 0, 0).expect("load_soundfont");
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

    println!(
        "frames={} channels={} sample_rate={}",
        res.frames, res.channels, res.sample_rate
    );
    println!(
        "wall={:?} audio={:.3}s realtime={:.3}x",
        wall,
        audio_secs,
        audio_secs / wall.as_secs_f64()
    );
    println!(
        "samples={} fnv1a64={:016x}",
        res.samples.len(),
        fnv1a64(&bytes)
    );
    println!("peak={:.9} rms={:.9}", peak, rms);

    if let Some(p) = raw_out {
        std::fs::write(p, &bytes).expect("write raw");
        println!("raw written: {p}");
    }
    if let Some(p) = wav_out {
        write_wav_f32(p, &res.samples, res.channels as u16, res.sample_rate).expect("write wav");
        println!("wav written: {p}");
    }
}
