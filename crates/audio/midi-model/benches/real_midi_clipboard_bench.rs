//! 真实 MIDI 大数据复制/粘贴基准（JSON 旧路径 vs 紧凑二进制优化路径）
//!
//! 复现性能日志里的「复制 2s / 粘贴 9s」：走带剪贴板用 JSON 序列化 100W+ 音符，
//! 复制端 `write!` 拼 1M 个音符对象、粘贴端 `serde_json` 解析 1M 个对象，是纯 CPU 悬崖。
//!
//! 优化路径：复用 `midi-model` 的紧凑二进制剪贴板（`encode_clipboard` / `decode_clipboard_chunks`），
//! 与钢琴卷帘已采用的快速路径一致——delta 变长 tick + 定长字段，粘贴端按音轨分组批量插入。
//!
//! 数据：默认读真实文件 `Ouranos - HDSQ & The Romanticist [v1.6.6].mid`（24,337,991 音符）。
//! 基准主体取「1,043,936 音符的选区」——正是性能日志里复制/粘贴的实际量级（复制 2s / 粘贴 9s）。
//! 另附全文件 24M 音符的二进制上下文行（JSON 在 24M 量级约需 3 分钟，不列入常规基准）。
//!
//! 运行：
//! ```bash
//! cargo bench -p lumino-midi-model --bench real_midi_clipboard_bench
//! ```
//!
//! 目标：复制 / 粘贴均 < 100ms（硬指标），优秀线 < 50ms（针对 1M 选区量级）。

use std::env;
use std::path::Path;
use std::time::Instant;

use lumino_midi_model::MidiDocument;
use lumino_midi_model::NoteEvent;
use lumino_midi_model::clipboard::{
    ClipRecord, decode_clipboard_records, encode_clipboard, parse_clipboard_header,
};

#[path = "real_midi_clipboard_bench/bin_path.rs"]
mod bin_path;
#[path = "real_midi_clipboard_bench/fixture.rs"]
mod fixture;
#[path = "real_midi_clipboard_bench/json_path.rs"]
mod json_path;

use bin_path::{copy_bin, paste_bin};
use fixture::{build_selection, compute_origin, load_doc, total_notes};
use json_path::{copy_json, paste_json};

/// 真实 MIDI 文件路径（用户提供的性能日志来源文件）。
const DEFAULT_MIDI: &str = r"D:\BM-DATA\MIDI File\Ouranos - HDSQ & The Romanticist [v1.6.6].mid";

/// 选区量级：对齐日志「已粘贴 1043936 个音符」（复制 2s / 粘贴 9s 的实际数据量）。
const SELECTION_NOTES: u32 = 1_043_936;

/// 目标阈值（硬指标）：复制 / 粘贴各自 < 100ms。
const TARGET_MS: f64 = 100.0;
/// 优秀线：< 50ms。
const EXCELLENT_MS: f64 = 50.0;

fn bar(label: &str, ms: f64) -> String {
    let mark = if ms < EXCELLENT_MS {
        "★★★ 优秀"
    } else if ms < TARGET_MS {
        "✓ 达标"
    } else {
        "✗ 超标"
    };
    format!("{label:<22} {:>10.2} ms  {mark}", ms)
}

/// 多次运行取平均（消除调度噪声，给出稳定可复现的数字）。
fn avg_ms(mut f: impl FnMut() -> f64, n: usize) -> f64 {
    let mut sum = 0.0;
    for _ in 0..n {
        sum += f();
    }
    sum / n as f64
}

/// 多次运行取最优（最贴近纯 CPU 成本，排除偶发调度抖动）。
fn min_ms(mut f: impl FnMut() -> f64, n: usize) -> f64 {
    let mut m = f64::INFINITY;
    for _ in 0..n {
        m = m.min(f());
    }
    m
}

fn main() {
    println!("=== Lumino 真实 MIDI 复制/粘贴基准（JSON vs 二进制）===");
    let (doc, src) = load_doc();
    let total = total_notes(&doc);
    let tracks = doc.track_count() as u16;
    let div = doc.division();
    println!("数据源: {src}");
    println!(
        "音轨数: {}  division: {}  总音符: {}",
        doc.track_count(),
        div,
        total
    );
    println!(
        "基准选区量级: {} 音符（对齐性能日志复制/粘贴实际数据量）",
        SELECTION_NOTES
    );
    println!();

    let sel = build_selection(&doc, SELECTION_NOTES);
    let origin = compute_origin(&sel);
    println!("选区 origin_tick={}  origin_key={}", origin.0, origin.1);
    println!();

    // —— JSON 旧路径（1M 选区，复现「复制 2s / 粘贴 9s」）——
    let copy_json_avg = avg_ms(|| copy_json(&sel, origin.0, origin.1).1, 3);
    let (json_str, _) = copy_json(&sel, origin.0, origin.1);
    let json_payload_mb = json_str.len() as f64 / (1024.0 * 1024.0);
    let paste_json_avg = avg_ms(
        || {
            let mut d = MidiDocument::empty_with_tracks(tracks, div);
            paste_json(&mut d, &json_str).1
        },
        3,
    );
    let (ins_json, _) = {
        let mut d = MidiDocument::empty_with_tracks(tracks, div);
        paste_json(&mut d, &json_str)
    };

    // —— 二进制优化路径（1M 选区，目标 < 100ms）——
    let copy_bin_avg = avg_ms(|| copy_bin(&sel, origin.0, origin.1).1, 7);
    let copy_bin_min = min_ms(|| copy_bin(&sel, origin.0, origin.1).1, 7);
    let (bin_bytes, _) = copy_bin(&sel, origin.0, origin.1);
    let bin_payload_mb = bin_bytes.len() as f64 / (1024.0 * 1024.0);
    let paste_bin_avg = avg_ms(
        || {
            let mut d = MidiDocument::empty_with_tracks(tracks, div);
            paste_bin(&mut d, &bin_bytes).1
        },
        7,
    );
    let paste_bin_min = min_ms(
        || {
            let mut d = MidiDocument::empty_with_tracks(tracks, div);
            paste_bin(&mut d, &bin_bytes).1
        },
        7,
    );
    let (ins_bin, _) = {
        let mut d = MidiDocument::empty_with_tracks(tracks, div);
        paste_bin(&mut d, &bin_bytes)
    };

    println!("（每组取多次运行平均；二进制附最优值）");
    println!("── 复制（序列化）──");
    println!("{}", bar("JSON write!", copy_json_avg));
    println!("{}", bar("二进制 encode", copy_bin_avg));
    println!("── 粘贴（解析+插入）──");
    println!("{}", bar("JSON parse", paste_json_avg));
    println!("{}", bar("二进制 decode", paste_bin_avg));
    println!(
        "二进制最优: 复制 {:.2} ms / 粘贴 {:.2} ms",
        copy_bin_min, paste_bin_min
    );
    println!();
    println!(
        "JSON 载荷 {:>8.2} MB | 二进制载荷 {:>8.2} MB | 压缩比 {:>5.1}x",
        json_payload_mb,
        bin_payload_mb,
        json_payload_mb / bin_payload_mb.max(1e-6)
    );
    println!(
        "JSON 复制+粘贴 合计 {:>9.2} ms | 二进制复制+粘贴 合计 {:>9.2} ms",
        copy_json_avg + paste_json_avg,
        copy_bin_avg + paste_bin_avg
    );
    println!("粘贴音符数: JSON={ins_json}  二进制={ins_bin}  (应相等，校验往返一致性)",);
    let pass = copy_bin_avg < TARGET_MS && paste_bin_avg < TARGET_MS && ins_bin == ins_json;
    println!(
        "结论(1M 选区): {}",
        if pass {
            "✓ 二进制路径复制/粘贴均 < 100ms，且往返音符数一致"
        } else {
            "✗ 未达标，需继续优化"
        }
    );

    // —— 全文件 24M 二进制上下文（不跑 JSON，避免 3 分钟级耗时）——
    println!();
    println!("── 全文件二进制上下文（{} 音符）──", total);
    let t0 = Instant::now();
    let all_bytes = encode_clipboard(
        (0..doc.track_count()).flat_map(|t| {
            doc.track_notes(t).iter().map(move |n| {
                ClipRecord::new(
                    n.start_tick,
                    n.end_tick - n.start_tick,
                    n.key,
                    n.velocity,
                    n.channel,
                    t as u16,
                )
            })
        }),
        total,
        div,
        0,
        0,
        0,
    );
    let enc_all = t0.elapsed().as_nanos() as f64 / 1e6;
    let mut doc_all = MidiDocument::empty_with_tracks(tracks, div);
    let (ins_all, dec_all) = paste_bin(&mut doc_all, &all_bytes);
    println!("{}", bar("二进制 encode", enc_all));
    println!("{}", bar("二进制 decode", dec_all));
    println!(
        "全文件二进制复制+粘贴 合计 {:>9.2} ms | 载荷 {:>8.2} MB | 插入 {}",
        enc_all + dec_all,
        all_bytes.len() as f64 / (1024.0 * 1024.0),
        ins_all
    );
    println!("=== 完成 ===");
}
