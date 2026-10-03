//! 音频渲染测试 — 使用 test-file 中的 MIDI 验证渲染管线
//! 含详细的计时和进度报告

use std::path::PathBuf;
use std::time::Instant;

use lumino_export::audio::config::{
    AudioBackendKind, AudioChannelMode, AudioInterpolation, AudioRenderConfig, ThreadMode,
};
use lumino_export::audio::render_audio;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("用法: render_test <midi_path> <sf2_path> [output.wav] [sample_rate]");
        eprintln!("  默认 sample_rate: 48000 (与 xsynth-render 默认一致)");
        std::process::exit(1);
    }

    let midi_path = PathBuf::from(&args[1]);
    let sf2_path = PathBuf::from(&args[2]);
    let output_path = if args.len() > 3 {
        PathBuf::from(&args[3])
    } else {
        PathBuf::from("render_output.wav")
    };
    let sample_rate: u32 = if args.len() > 4 {
        args[4].parse().unwrap_or_else(|_| {
            eprintln!("错误: sample_rate 必须是整数 (如 44100, 48000)");
            std::process::exit(1);
        })
    } else {
        48000
    };

    if !midi_path.exists() {
        eprintln!("MIDI 不存在: {:?}", midi_path);
        std::process::exit(1);
    }
    if !sf2_path.exists() {
        eprintln!("SF2 不存在: {:?}", sf2_path);
        std::process::exit(1);
    }

    let midi_size_mb = std::fs::metadata(&midi_path)
        .map(|m| m.len() as f64 / 1_048_576.0)
        .unwrap_or(0.0);
    let sf2_size_mb = std::fs::metadata(&sf2_path)
        .map(|m| m.len() as f64 / 1_048_576.0)
        .unwrap_or(0.0);

    println!("════════════════════════════════════════════════════");
    println!("  Lumino 音频渲染测试");
    println!("════════════════════════════════════════════════════");
    println!("  MIDI:  {:?} ({:.2} MB)", midi_path, midi_size_mb);
    println!("  SF2:   {:?} ({:.2} MB)", sf2_path, sf2_size_mb);
    println!("  输出:  {:?}", output_path);

    // 先分析 MIDI 基本信息
    println!("\n── 分析 MIDI ──");
    match std::fs::read(&midi_path) {
        Ok(data) => {
            match lumino_midi_loader::StreamingMidiPlayer::from_bytes(&data) {
                Ok(player) => {
                    let tempos: Vec<(u32, f32)> = player.tempo_changes().to_vec();
                    println!("  音轨数:   {}", player.track_count());
                    println!("  总 ticks: {}", player.total_ticks());
                    println!("  PPQN:     {}", player.ppqn());
                    println!("  速度变化: {} 条", tempos.len());
                    for (i, &(t, bpm)) in tempos.iter().take(10).enumerate() {
                        println!("    [{i}] tick={t} → {bpm:.1} BPM");
                    }
                    if tempos.len() > 10 {
                        println!("    ... 还有 {} 条", tempos.len() - 10);
                    }

                    // 快速扫一遍事件数
                    match lumino_midi_loader::StreamingMidiPlayer::from_bytes(&data) {
                        Ok(mut player2) => {
                            let mut event_count = 0u64;
                            let mut note_on = 0u64;
                            while let Some((_t, _tr, kind)) = player2.next_event() {
                                event_count += 1;
                                if let midly::TrackEventKind::Midi {
                                    message: midly::MidiMessage::NoteOn { .. },
                                    ..
                                } = kind
                                {
                                    note_on += 1;
                                }
                            }
                            println!("  总事件:   {}", event_count);
                            println!("  音符 On:  {}", note_on);
                        }
                        Err(e) => println!("  MIDI 事件扫描失败: {e}"),
                    }

                    // 估计时长
                    let t = lumino_export::audio::tick_conv::TickToTime::new(tempos, player.ppqn());
                    let est_secs = t.tick_to_seconds(player.total_ticks());
                    println!("  预估时长: {:.1}s ({:.1}分)", est_secs, est_secs / 60.0);
                }
                Err(e) => println!("  MIDI 分析失败: {e}"),
            }
        }
        Err(e) => println!("  读取 MIDI 失败: {e}"),
    }

    // ── 配置 ──
    let config = AudioRenderConfig {
        midi_path: midi_path.clone(),
        soundfonts: vec![sf2_path],
        output_path: output_path.clone(),
        sample_rate,
        channels: AudioChannelMode::Stereo,
        layer_limit: Some(64),
        channel_threading: ThreadMode::Auto,
        key_threading: ThreadMode::Auto,
        interpolation: AudioInterpolation::Linear,
        apply_limiter: false,
        disable_fade_out: false,
        linear_envelope: false,
        audio_codec: lumino_export::audio::codec::AudioCodec::Pcm,
        audio_bitrate: 320,
        ignore_program_changes: false,
        velocity_low: 0,
        velocity_high: 127,
        filter_velocity: false,
        key_low: 0,
        key_high: 127,
        filter_key: false,
        note_force_end_delay: 0,
        backend: AudioBackendKind::Cpu,
        // 顺手补：REND-002 新增字段，此 bin 漏同步（0 = 单端口/Midi 16 通道，对齐 config.rs 默认值）
        midi_max_port: 0,
        // 顺手补：PREF-002 新增字段，此 bin 漏同步（与本 issue 无关，默认值对齐 config.rs）
        block_frames: 256,
        progress_callback: None,
        control: None,
    };

    // ── 流式渲染 ──
    println!("\n── 开始流式渲染 ──");
    let start = Instant::now();
    let render_result = render_audio(&config);
    let elapsed = start.elapsed();

    match render_result {
        Ok(()) => {
            let out_size = std::fs::metadata(&output_path)
                .map(|m| {
                    let bytes = m.len();
                    let mb = bytes as f64 / 1_048_576.0;
                    let duration_secs = bytes as f64 / (config.sample_rate as f64 * 8.0);
                    (bytes, mb, duration_secs)
                })
                .unwrap_or((0, 0.0, 0.0));

            println!("\n════════════════════════════════════════════════════");
            println!("  渲染完成 ✓");
            println!("════════════════════════════════════════════════════");
            println!("  渲染耗时: {:?} ({:.2}s)", elapsed, elapsed.as_secs_f64());
            println!(
                "  输出:     {:?} ({} bytes / {:.2} MB)",
                output_path, out_size.0, out_size.1
            );
            if out_size.2 > 0.0 {
                println!(
                    "  音频时长: ~{:.1}s ({:.1}分)",
                    out_size.2,
                    out_size.2 / 60.0
                );
            }
            println!("  采样率:   {} Hz", config.sample_rate);
            println!("  声道:     立体声 (32-bit float)");
        }
        Err(e) => {
            eprintln!("\n✗ 渲染失败: {e}");
            std::process::exit(1);
        }
    }
}
