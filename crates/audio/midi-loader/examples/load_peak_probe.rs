//! MIDI 加载峰值 / 等价性探针（性能卡验收用）
//!
//! 走**生产加载入口** `load_parsed_midi`，输出一组稳定的 `KEY=VALUE` 行：
//! 改动前后用同一份文件跑同一命令，`diff` 即可证明文档逐项等价。
//! 峰值内存由外部脚本读取（Windows：`scripts/measure-midi-load.ps1`，
//! 用 OS 的 `PeakPagefileUsage` / `PeakWorkingSetSize`，不是采样）。
//!
//! # 用法
//!
//! ```text
//! cargo run --release -p lumino-midi-loader --example load_peak_probe -- <midi-path>
//! pwsh scripts/measure-midi-load.ps1 -Midi <midi-path> -Reps 3
//! ```
//!
//! 前后对照：设置 `LUMINO_MIDI_INPUT=read` 可强制走整份读取（改造前行为）。
//!
//! # 指纹口径
//!
//! 音符按「每轨文件序」哈希（顺序敏感）；其余事件类别**先排序再哈希**——各类事件
//! 由 rayon 并行解析后按 tick 稳定排序，**同 tick 的跨轨事件顺序在运行之间不保证
//! 一致**（改造前即如此），顺序敏感的哈希无法用于等价判定。
//!
//! # 输出字段
//!
//! * `FILE_BYTES` / `LOADER_MS` —— 文件大小与端到端加载耗时
//! * `NOTES` / `TRACKS` / `DIVISION` / `TOTAL_TICKS` —— 文档规模
//! * `*_N` + `*_HASH` —— 各类事件的条数与内容哈希（逐项等价用）
//! * `DOC_NOTE_MIB` —— 音符部分的理论常驻字节（12 B/音符）

use std::time::Instant;

use lumino_midi_loader::MidiDocument;
use lumino_midi_loader::loader::load_parsed_midi;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 滚动哈希：逐字段混入。
#[inline]
fn mix(h: &mut u64, v: u64) {
    *h ^= v;
    *h = h.wrapping_mul(0x0000_0100_0000_01B3);
}

fn hash_bytes(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        mix(&mut h, b as u64);
    }
    h
}

fn hash_str(mut h: u64, s: &str) -> u64 {
    for &b in s.as_bytes() {
        mix(&mut h, b as u64);
    }
    h
}

/// 文档指纹：音符/控制事件/文本类事件/音轨名/端口。
fn print_fingerprint(doc: &MidiDocument) {
    let mut notes_h = FNV_OFFSET;
    let mut per_track_sum: u64 = 0;
    let mut per_track_max: usize = 0;
    for track in &doc.notes {
        per_track_sum += track.len() as u64;
        per_track_max = per_track_max.max(track.len());
        for n in track.iter() {
            mix(
                &mut notes_h,
                ((n.start_tick as u64) << 32) | n.end_tick as u64,
            );
            mix(
                &mut notes_h,
                ((n.key as u64) << 24)
                    | ((n.velocity as u64) << 16)
                    | ((n.release_velocity as u64) << 8)
                    | n.channel as u64,
            );
        }
    }

    // 同 tick 跨轨事件顺序不稳定（rayon 到达序 + 按 tick 稳定排序），故排序后哈希。
    let mut tempo: Vec<(u32, u32)> = doc
        .tempo_changes
        .iter()
        .map(|(tick, bpm)| (*tick, bpm.to_bits()))
        .collect();
    tempo.sort_unstable();

    let mut timesig: Vec<(u32, u8, u8)> = doc.time_signatures.clone();
    timesig.sort_unstable();

    let mut keysig: Vec<(u32, i8, bool)> = doc.key_signatures.clone();
    keysig.sort_unstable();

    let mut cc: Vec<(u32, u16, u8, u8, u16)> = doc
        .control_events
        .iter()
        .map(|e| (e.tick, e.track, e.kind, e.channel, e.param))
        .collect();
    cc.sort_unstable();

    // 歌词 / 标记 / 文本类元事件统一成 (tick, track, tag, 字节)
    let mut texts: Vec<(u32, u16, u8, Vec<u8>)> = Vec::new();
    for (tick, track, bytes) in doc.lyrics.iter() {
        texts.push((*tick, *track, 0x05, bytes.clone()));
    }
    for (tick, track, bytes) in doc.markers.iter() {
        texts.push((*tick, *track, 0x06, bytes.clone()));
    }
    for (tick, track, ty, bytes) in doc.text_events.iter() {
        texts.push((*tick, *track, *ty, bytes.clone()));
    }
    texts.sort_unstable();

    let mut sysex: Vec<(u32, u16, Vec<u8>)> = doc
        .sys_ex
        .iter()
        .map(|(tick, track, bytes)| (*tick, *track, bytes.clone()))
        .collect();
    sysex.sort_unstable();

    let mut tempo_h = FNV_OFFSET;
    for (tick, bits) in &tempo {
        mix(&mut tempo_h, *tick as u64);
        mix(&mut tempo_h, *bits as u64);
    }

    let mut ts_h = FNV_OFFSET;
    for (tick, num, den) in &timesig {
        mix(&mut ts_h, *tick as u64);
        mix(&mut ts_h, ((*num as u64) << 8) | *den as u64);
    }

    let mut ks_h = FNV_OFFSET;
    for (tick, sharps, minor) in &keysig {
        mix(&mut ks_h, *tick as u64);
        mix(&mut ks_h, ((*sharps as i64 as u64) << 1) | *minor as u64);
    }

    let mut cc_h = FNV_OFFSET;
    for (tick, track, kind, channel, param) in &cc {
        mix(&mut cc_h, *tick as u64);
        mix(&mut cc_h, *track as u64);
        mix(
            &mut cc_h,
            ((*kind as u64) << 24) | ((*channel as u64) << 16) | *param as u64,
        );
    }

    let mut text_h = FNV_OFFSET;
    for (tick, track, tag, bytes) in &texts {
        mix(&mut text_h, *tick as u64);
        mix(&mut text_h, ((*track as u64) << 8) | *tag as u64);
        text_h = hash_bytes(text_h, bytes);
    }

    let mut sysex_h = FNV_OFFSET;
    for (tick, track, bytes) in &sysex {
        mix(&mut sysex_h, *tick as u64);
        mix(&mut sysex_h, *track as u64);
        sysex_h = hash_bytes(sysex_h, bytes);
    }

    let mut names_h = FNV_OFFSET;
    for name in &doc.track_names {
        match name {
            Some(n) => names_h = hash_str(names_h, n),
            None => mix(&mut names_h, 0xFF),
        }
    }

    let mut ports_h = FNV_OFFSET;
    for port in &doc.track_ports {
        mix(&mut ports_h, *port as u64);
    }

    let input_mode = std::env::var("LUMINO_MIDI_INPUT").unwrap_or_else(|_| "mmap".to_string());
    println!("INPUT_MODE={input_mode}");
    println!("NOTES={per_track_sum}");
    println!("TRACKS={}", doc.track_count);
    println!("DIVISION={}", doc.division);
    println!("TOTAL_TICKS={}", doc.total_ticks);
    println!("PER_TRACK_MAX={per_track_max}");
    println!("NOTES_HASH={notes_h:016x}");
    println!("TEMPO_N={} TEMPO_HASH={tempo_h:016x}", tempo.len());
    println!("TIMESIG_N={} TIMESIG_HASH={ts_h:016x}", timesig.len());
    println!("KEYSIG_N={} KEYSIG_HASH={ks_h:016x}", keysig.len());
    println!("CC_N={} CC_HASH={cc_h:016x}", cc.len());
    println!(
        "LYRICS_N={} MARKERS_N={} TEXT_N={} TEXT_ALL_N={} TEXT_HASH={text_h:016x}",
        doc.lyrics.len(),
        doc.markers.len(),
        doc.text_events.len(),
        texts.len()
    );
    println!("SYSEX_N={} SYSEX_HASH={sysex_h:016x}", sysex.len());
    println!("NAMES_HASH={names_h:016x} PORTS_HASH={ports_h:016x}");
    println!(
        "DOC_NOTE_MIB={:.1}",
        (per_track_sum as f64 * std::mem::size_of::<lumino_midi_loader::NoteEvent>() as f64)
            / (1024.0 * 1024.0)
    );
}

#[tokio::main]
async fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("用法: load_peak_probe <midi-path>");
        std::process::exit(2);
    };

    let meta = std::fs::metadata(&path).expect("读取文件元信息失败");
    println!("FILE_BYTES={}", meta.len());

    let started = Instant::now();
    let parsed = match load_parsed_midi(std::path::PathBuf::from(&path), None).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("加载失败: {e}");
            std::process::exit(1);
        }
    };
    println!("LOADER_MS={}", started.elapsed().as_millis());

    let Some(doc) = parsed.document.as_deref() else {
        eprintln!("加载结果不含 MidiDocument");
        std::process::exit(1);
    };
    print_fingerprint(doc);
}
