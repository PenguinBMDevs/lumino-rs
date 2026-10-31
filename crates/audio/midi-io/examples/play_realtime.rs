//! REF-LGS-012 验收工具：LGS 实时事件注入压测。
//!
//! 走**真实应用路径**（`ApiKind::Lgs` → `OutputConnection`）按 MIDI 时间线逐事件
//! 注入，周期采样 `GpuBufferStats`，结束打印可 diff 的汇总行。
//!
//! 用法：
//! ```text
//! cargo run --release -p lumino-midi-io --example play_realtime -- \
//!     <midi> <sf2> <seconds> [block] [--batch]
//! ```
//! - 默认逐事件（应用现状路径）；
//! - `--batch` 走 `OutputConnection::send_raw_batch`（REF-LGS-012 新增的批量入口）。
//!
//! 输出：每 250 ms 一行状态 + 一行 `SUMMARY`，用于「改动前/后」对照
//! （`underruns` / `dropped_note_ons` / `max_governor` 三项为主）。

use std::time::{Duration, Instant};

use lumino_gpu_synth::MidiFile;
use lumino_gpu_synth::midi::MidiEvent;
use lumino_midi_io::{ApiKind, new_api_with_options};

const RATE: u32 = 48_000;
/// 调度器每次“醒来”送出的事件时间窗（2 ms）。应用侧的播放调度器也是按小片推进。
const SLICE_SAMPLES: u32 = RATE / 500;

fn usage() -> ! {
    eprintln!("用法: play_realtime <midi> <sf2> <seconds> [block] [--batch]");
    std::process::exit(2);
}

/// `MidiEvent` → MIDI 线协议 3 字节（供批量入口使用）。
fn raw_bytes(ch: u8, event: MidiEvent) -> [u8; 3] {
    let ch = ch & 0x0F;
    match event {
        MidiEvent::NoteOn { key, vel } => [0x90 | ch, key & 0x7F, vel & 0x7F],
        MidiEvent::NoteOff { key } => [0x80 | ch, key & 0x7F, 0],
        MidiEvent::ControlChange { controller, value } => {
            [0xB0 | ch, controller & 0x7F, value & 0x7F]
        }
        MidiEvent::ProgramChange { program } => [0xC0 | ch, program & 0x7F, 0],
        MidiEvent::PitchBend { value } => {
            [0xE0 | ch, (value & 0x7F) as u8, ((value >> 7) & 0x7F) as u8]
        }
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let batch = args.iter().any(|a| a == "--batch");
    let unpaced = args.iter().any(|a| a == "--unpaced");
    args.retain(|a| a != "--batch" && a != "--unpaced");
    if args.len() < 3 {
        usage();
    }
    let midi_path = &args[0];
    let sf2 = &args[1];
    let seconds: f64 = args[2].parse().unwrap_or(30.0);
    let block: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(512);

    let midi = MidiFile::load(midi_path, RATE).expect("MidiFile::load");
    let events = &midi.sequence.events;
    println!(
        "fixture={} events={} end_sample={} block={} path={}",
        midi_path,
        events.len(),
        midi.sequence.end_sample,
        block,
        if batch { "batch" } else { "per-event" }
    );

    let api = new_api_with_options(
        &ApiKind::Lgs {
            soundfont_path: sf2.into(),
            sample_rate: RATE,
            block_size: block,
            max_voices_per_key: 0,
            max_voices: 0,
            soft_nps_gate: false,
            use_sinc: false,
            velocity_filter_threshold: 0,
            audio_output_device: None,
            midi_max_port: 0,
        },
        None,
    )
    .expect("LGS 初始化失败（音频设备/GPU）");
    let mut out = api.open_output(0).expect("open_output");
    out.reset_playback_stats().ok();

    // `--unpaced`：不按时间线节流，尽可能快地灌满注入通道——测的是**发送端**
    // 吞吐（ns/event），不受调度器睡眠影响；引擎侧的丢弃由 Governor 决定，
    // 两项都要看。
    if unpaced {
        let t = Instant::now();
        let mut raws: Vec<[u8; 3]> = Vec::with_capacity(4096);
        for e in events {
            if batch {
                raws.push(raw_bytes(e.channel(), e.event()));
                if raws.len() == 4096 {
                    let _ = out.send_raw_batch(&raws);
                    raws.clear();
                }
            } else {
                let ch = u16::from(e.channel());
                let _ = match e.event() {
                    MidiEvent::NoteOn { key, vel } => out.note_on(ch, key, vel),
                    MidiEvent::NoteOff { key } => out.note_off(ch, key, 0),
                    MidiEvent::ControlChange { controller, value } => {
                        out.control_change(ch, controller, value)
                    }
                    MidiEvent::ProgramChange { program } => out.program_change(ch, program),
                    MidiEvent::PitchBend { value } => {
                        let bend = f32::from(value) / 8192.0 - 1.0;
                        out.pitch_bend(ch, bend)
                    }
                };
            }
        }
        if !raws.is_empty() {
            let _ = out.send_raw_batch(&raws);
        }
        let elapsed = t.elapsed();
        let n = events.len() as f64;
        let s = api.gpu_buffer_stats().unwrap_or_default();
        println!(
            "UNPACED path={} sent={} elapsed_ms={:.3} ns_per_event={:.1} underruns={} dropped_note_ons={}",
            if batch { "batch" } else { "per-event" },
            events.len(),
            elapsed.as_secs_f64() * 1000.0,
            elapsed.as_secs_f64() * 1e9 / n,
            s.underruns,
            s.dropped_note_ons
        );
        return;
    }

    let total_samples = (seconds * f64::from(RATE)) as u64;
    let start = Instant::now();
    let mut idx = 0usize;
    let mut sent = 0u64;
    let mut max_gov = 0u64;
    let mut max_voices = 0u64;
    let mut last_poll = start;

    while (start.elapsed().as_secs_f64() * f64::from(RATE)) as u64 <= total_samples
        && idx < events.len()
    {
        let now = (start.elapsed().as_secs_f64() * f64::from(RATE)) as u32;
        let horizon = now.saturating_add(SLICE_SAMPLES);
        let mut raws: Vec<[u8; 3]> = Vec::with_capacity(64);
        while idx < events.len() && events[idx].sample <= horizon {
            let e = events[idx];
            if batch {
                raws.push(raw_bytes(e.channel(), e.event()));
            } else {
                let ch = u16::from(e.channel());
                // 与 LgsOutputConn 的逐事件路径一一对应。
                let _ = match e.event() {
                    MidiEvent::NoteOn { key, vel } => out.note_on(ch, key, vel),
                    MidiEvent::NoteOff { key } => out.note_off(ch, key, 0),
                    MidiEvent::ControlChange { controller, value } => {
                        out.control_change(ch, controller, value)
                    }
                    MidiEvent::ProgramChange { program } => out.program_change(ch, program),
                    MidiEvent::PitchBend { value } => {
                        let bend = f32::from(value) / 8192.0 - 1.0;
                        out.pitch_bend(ch, bend)
                    }
                };
            }
            idx += 1;
            sent += 1;
        }
        if !raws.is_empty() {
            let _ = out.send_raw_batch(&raws);
        }

        if last_poll.elapsed() >= Duration::from_millis(250)
            && let Some(s) = api.gpu_buffer_stats()
        {
            max_gov = max_gov.max(s.governor_level);
            max_voices = max_voices.max(s.voice_count);
            println!(
                "t={:.1}s sent={} voices={} queue_ms={:.1} load={:.3} underruns={} dropped={} gov={}",
                start.elapsed().as_secs_f64(),
                sent,
                s.voice_count,
                s.queue_ms,
                s.average_render_load,
                s.underruns,
                s.dropped_note_ons,
                s.governor_level
            );
            last_poll = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    // 让尾音播完再收尾（统计仍在更新）。
    std::thread::sleep(Duration::from_millis(500));
    let s = api.gpu_buffer_stats().unwrap_or_default();
    println!(
        "SUMMARY path={} block={} sent={} underruns={} dropped_note_ons={} max_governor={} max_voices={}",
        if batch { "batch" } else { "per-event" },
        block,
        sent,
        s.underruns,
        s.dropped_note_ons,
        max_gov,
        max_voices
    );
}
