//! 剪贴板复制→粘贴性能基准（紧凑二进制载体）
//!
//! 覆盖 10W / 100W / 1000W 音符的复制（编码）与粘贴（解码+插入）时空双指标。
//! 运行：`cargo bench -p lumino-midi-model`（本 bench 以 `harness=false` 手动计时）。
//!
//! 内存口径：报道「剪贴板载荷字节数」为复制/粘贴期间的**额外**堆内存（不含两端文档已存储的音符）。

use std::time::Instant;

use lumino_midi_model::clipboard::{
    decode_clipboard_chunks, encode_clipboard, parse_clipboard_header, record_to_note_event,
    ClipRecord, ClipMeta,
};
use lumino_midi_model::MidiDocument;

/// 生成 N 条按 tick 升序、密集排布的音符记录流（不物化大 `Vec`）。
/// tick_offset = i（相邻 delta=1 → varint 1 字节），length 短，key/vel/ch/track 固定。
fn record_stream(n: u32) -> impl Iterator<Item = ClipRecord> {
    (0..n).map(|i| {
        ClipRecord::new(
            i,                  // tick_offset 升序
            (i % 4 + 1) as u32, // length 1..4
            (i % 100) as u8,
            100,
            0,
            0,
        )
    })
}

fn bench_one(n: usize) {
    let division_src = 480u16;
    let division_dst = 480u16; // 同 PPQN：ratio=1，零缩放
    let ratio = if division_src == division_dst {
        1.0
    } else {
        division_dst as f64 / division_src as f64
    };

    // —— 复制（编码）——
    let t0 = Instant::now();
    let bytes = encode_clipboard(record_stream(n as u32), n, division_src, 0, 0, 0);
    let enc_ms = t0.elapsed().as_nanos() as f64 / 1e6;

    // 预解析头部，供解码回调还原 NoteEvent（origin / division）
    let meta: ClipMeta = parse_clipboard_header(&bytes).expect("头部解析失败");

    // —— 粘贴（解码 + 分块插入，不物化全量 Vec）——
    let mut doc = MidiDocument::empty_with_tracks(1, division_dst);
    let chunk = 100_000usize;
    let t1 = Instant::now();
    decode_clipboard_chunks(&bytes, chunk, |recs| {
        let notes: Vec<_> = recs
            .iter()
            .map(|r| record_to_note_event(r, &meta, ratio))
            .collect();
        doc.batch_insert_sorted_notes_with_ids(0, notes);
    })
    .expect("decode 失败");
    let dec_ms = t1.elapsed().as_nanos() as f64 / 1e6;

    let payload_mb = bytes.len() as f64 / (1024.0 * 1024.0);
    let total_ms = enc_ms + dec_ms;

    println!(
        "N={:>10} | 载荷 {:>7.2} MB | 编码 {:>9.2} ms | 解码+插入 {:>9.2} ms | 合计 {:>9.2} ms | 目标<100MB/<500ms: {}",
        n,
        payload_mb,
        enc_ms,
        dec_ms,
        total_ms,
        if payload_mb < 100.0 && total_ms < 500.0 {
            "达标"
        } else {
            "未达标"
        }
    );
    std::hint::black_box(&doc);
}

fn main() {
    println!("=== Lumino 剪贴板复制/粘贴基准（紧凑二进制）===");
    for &n in &[10_000usize, 100_000, 1_000_000, 10_000_000] {
        bench_one(n);
    }
    println!("=== 完成 ===");
}
