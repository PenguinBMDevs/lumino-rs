//! 一次性 GPU 渲染 profiling：把成本拆成三段
//!   1. wgpu 设备 + 管线创建
//!   2. SF2 解析
//!   3. 预热（扫 MIDI + 重采样 + 上传 samples_chunks）
//!   4. 离线渲染循环（每块 apply/upload/dispatch/readback）
//!
//! 用法: profile_gpu <midi> <sf2> [frames]
//! 配合环境变量 LUMINO_PROFILE=1 输出每 25 块的分阶段耗时。

use lumino_gpu_synth::{GpuSynth, SynthConfig};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("用法: profile_gpu <midi> <sf2> [frames]");
        std::process::exit(2);
    }
    let midi = args[1].clone();
    let sf2 = args[2].clone();
    let frames: u64 = args
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(48_000 * 20);
    let block_size: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(512);

    let config = SynthConfig {
        sample_rate: 48_000,
        block_size,
        max_voices: 0,
        max_voices_per_key: 0,
        show_progress: false,
        ..SynthConfig::default()
    };

    println!("==== A: 初始化 / 解析 / 预热 ====");
    let t = Instant::now();
    let mut s1 = GpuSynth::new(config.clone()).expect("GpuSynth::new");
    println!(
        "[A1] GpuSynth::new (wgpu device+pipelines) : {:?}",
        t.elapsed()
    );
    let t = Instant::now();
    s1.load_soundfont(&sf2, 0, 0).expect("load_soundfont");
    println!(
        "[A2] load_soundfont (解析 SF2)              : {:?}",
        t.elapsed()
    );
    let t = Instant::now();
    s1.prewarm_midi_file(&midi).expect("prewarm_midi_file");
    println!(
        "[A3] prewarm_midi_file (扫MIDI+重采样+上传) : {:?}",
        t.elapsed()
    );
    drop(s1);

    println!("\n==== B: 离线渲染 {frames} 帧 ====");
    let mut s2 = GpuSynth::new(config).expect("GpuSynth::new");
    s2.load_soundfont(&sf2, 0, 0).expect("load_soundfont");
    let t = Instant::now();
    let res = s2
        .render_midi_frames(&midi, frames)
        .expect("render_midi_frames");
    println!(
        "[B] render_midi_frames : {:?} frames={} ch={} sr={} samples={}",
        t.elapsed(),
        res.frames,
        res.channels,
        res.sample_rate,
        res.samples.len()
    );
}
