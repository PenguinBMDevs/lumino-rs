//! 基准场景：二进制优化路径的复制（流式 `encode_clipboard`）与粘贴（分块解码 + 按轨连续刷入）
//!
//! 由 `real_midi_clipboard_bench.rs` 拆出，目标 1M 选区复制 / 粘贴均 < 100ms。

use super::*;

/// ── 二进制优化路径：复制（构建 `ClipRecord` 流 + 流式 `encode_clipboard`）──
///
/// 与钢琴卷帘 `build_clipboard_binary` 同构：每音符 `ClipRecord`（delta 变长 tick + 定长字段），
/// 显式传入 count 精确预分配 `Vec<u8>`，避免 filter_map/flat_map 的 size_hint=0 反复 realloc 悬崖。
pub fn copy_bin(sel: &[(usize, NoteEvent)], origin_tick: u32, origin_key: u8) -> (Vec<u8>, f64) {
    let t0 = Instant::now();
    // 选区已物化为 Vec，故 ClipRecord 也直接物化（与应用侧 collect_selected_notes 同构），
    // 换来精确 size_hint 与无闭包链开销的编码。
    let records: Vec<ClipRecord> = sel
        .iter()
        .map(|(t, n)| {
            ClipRecord::new(
                n.start_tick - origin_tick,
                n.end_tick - n.start_tick,
                (n.key as i32 - origin_key as i32).max(0) as u8,
                n.velocity,
                n.channel,
                *t as u16,
            )
        })
        .collect();
    let n = records.len();
    let bytes = encode_clipboard(records.into_iter(), n, 480, origin_tick, origin_key, 0);
    let ms = t0.elapsed().as_nanos() as f64 / 1e6;
    (bytes, ms)
}

/// ── 二进制优化路径：粘贴（分块解码 + 按音轨连续刷入 + 已排序批量插入）──
///
/// 关键优化：解码单遍完成，**按音轨连续 flush**（同轨子序列天然 tick 升序），直接走
/// `batch_insert_sorted_notes_with_ids` 免排序、**无 per-note HashMap 哈希**。
pub fn paste_bin(doc: &mut MidiDocument, bytes: &[u8]) -> (usize, f64) {
    let meta = parse_clipboard_header(bytes).expect("头部解析失败");
    let t0 = Instant::now();
    let mut cur_track: Option<usize> = None;
    let mut cur_vec: Vec<NoteEvent> = Vec::new();
    let mut inserted = 0usize;
    // 同 PPQN（ratio=1）：纯整数快路径，就地构造 NoteEvent 免去中间结构体二次构造。
    let origin_tick = meta.origin_tick;
    let origin_key = meta.origin_key as u32;
    decode_clipboard_records(
        bytes,
        |tick_offset, length, key_offset, velocity, channel, track| {
            let track = track as usize;
            if cur_track != Some(track) {
                if let Some(t) = cur_track {
                    inserted += doc
                        .batch_insert_sorted_notes_with_ids(t, std::mem::take(&mut cur_vec))
                        .len();
                }
                cur_track = Some(track);
            }
            let start = origin_tick.saturating_add(tick_offset);
            let note = NoteEvent::new(
                start,
                start.saturating_add(length),
                (origin_key + key_offset as u32).min(127) as u8,
                velocity,
                channel,
            );
            cur_vec.push(note);
        },
    )
    .expect("decode 失败");
    if let Some(t) = cur_track {
        inserted += doc.batch_insert_sorted_notes_with_ids(t, cur_vec).len();
    }
    let ms = t0.elapsed().as_nanos() as f64 / 1e6;
    (inserted, ms)
}
