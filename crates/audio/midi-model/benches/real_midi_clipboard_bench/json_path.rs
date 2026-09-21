//! 基准场景：JSON 旧路径的复制（`write!` 拼串）与粘贴（`serde_json` 解析 + 按轨批量插入）
//!
//! 由 `real_midi_clipboard_bench.rs` 拆出，复现 1M 选区「复制 2s / 粘贴 9s」的对照基线。

use super::*;

/// ── JSON 旧路径：复制（手写 `write!` 拼 JSON 字符串）──
///
/// 与 `arrangement_ops::clipboard::write_arrangement_clipboard` 同构：1M 音符对象逐条格式化。
/// 计时区间覆盖整段字符串构建（即复制端真实 CPU 成本）。
pub fn copy_json(sel: &[(usize, NoteEvent)], origin_tick: u32, origin_key: u8) -> (String, f64) {
    let ot = origin_tick as f32;
    let ok = origin_key as u16;
    let t0 = Instant::now();
    let mut s = String::with_capacity(sel.len().saturating_mul(48) + 180);
    use std::fmt::Write as _;
    let _ = write!(
        s,
        "{{\"type\":\"arrangement\",\"origin_tick\":{ot},\"origin_key\":{ok},\"division\":480,\"notes\":["
    );
    let mut first = true;
    for (t, n) in sel {
        let tick = (n.start_tick as f32 - ot).max(0.0);
        let key = (n.key as i32 - ok as i32).max(0) as u16;
        let length = (n.end_tick - n.start_tick) as f32;
        if !first {
            s.push(',');
        }
        first = false;
        let _ = write!(
            s,
            "{{\"tick\":{tick},\"key\":{key},\"length\":{length},\"velocity\":{},\"channel\":{},\"track\":{t}}}",
            n.velocity, n.channel
        );
    }
    s.push(']');
    s.push('}');
    let ms = t0.elapsed().as_nanos() as f64 / 1e6;
    (s, ms)
}

/// ── JSON 旧路径：粘贴（`serde_json` 解析 + 按轨批量插入）──
pub fn paste_json(doc: &mut MidiDocument, text: &str) -> (usize, f64) {
    let t0 = Instant::now();
    let value: serde_json::Value = serde_json::from_str(text).expect("JSON 解析失败");
    let notes = value
        .get("notes")
        .and_then(|v| v.as_array())
        .expect("notes 缺失");
    let mut by_track: std::collections::HashMap<usize, Vec<NoteEvent>> =
        std::collections::HashMap::new();
    for item in notes {
        let tick = item.get("tick").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let key = item.get("key").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
        let length = item.get("length").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let velocity = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
        let channel = item.get("channel").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
        let track = item.get("track").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let note = NoteEvent::new(tick as u32, (tick + length) as u32, key, velocity, channel);
        by_track.entry(track).or_default().push(note);
    }
    let mut inserted = 0usize;
    for (track, notes) in by_track {
        inserted += doc.batch_insert_notes_with_ids(track, notes).len();
    }
    let ms = t0.elapsed().as_nanos() as f64 / 1e6;
    (inserted, ms)
}
