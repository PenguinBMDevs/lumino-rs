//! 基准 fixture/loader：真实 MIDI 加载、合成回退、选区与 origin 预计算
//!
//! 由 `real_midi_clipboard_bench.rs` 拆出，供 JSON / 二进制两条基准路径共用。

use super::*;

/// 加载真实 MIDI；文件缺失则回退合成数据（合成 1,043,936 音符选区 + 等价全量）。
pub fn load_doc() -> (MidiDocument, String) {
    let path = env::var("LUMINO_BENCH_MIDI").unwrap_or_else(|_| DEFAULT_MIDI.to_string());
    if Path::new(&path).exists() {
        match MidiDocument::from_notes_file(&path, None) {
            Ok(doc) => {
                let n: u64 = (0..doc.track_count())
                    .map(|t| doc.track_note_count(t as u16))
                    .sum();
                return (doc, format!("real:{path} (notes={n})"));
            }
            Err(e) => eprintln!("⚠ 加载真实 MIDI 失败: {e}，回退合成数据"),
        }
    }
    (synth_doc(), "synthetic".into())
}

/// 合成文档：跨 16 轨铺满 ~1,043,936 音符（升序 tick，模拟黑乐谱密集排布）。
fn synth_doc() -> MidiDocument {
    let per = SELECTION_NOTES / 16;
    let mut doc = MidiDocument::empty_with_tracks(16, 480);
    for t in 0..16u16 {
        let mut notes: Vec<NoteEvent> = Vec::with_capacity(per as usize);
        for i in 0..per {
            let start = i * 4;
            let key = ((i * 7) % 128) as u8;
            notes.push(NoteEvent::new(start, start + 2, key, 100, (t % 16) as u8));
        }
        doc.batch_insert_sorted_notes_with_ids(t as usize, notes);
    }
    doc
}

/// 取前 `n` 个音符作为「选区」（真实数据顺序：逐轨、轨内按 tick 升序）。
/// 这正是 `arrangement_ops::clipboard::collect_selected_notes_for_clipboard` 产出的形态。
pub fn build_selection(doc: &MidiDocument, n: u32) -> Vec<(usize, NoteEvent)> {
    let mut sel: Vec<(usize, NoteEvent)> = Vec::with_capacity(n as usize);
    for t in 0..doc.track_count() {
        for note in doc.track_notes(t).iter() {
            sel.push((t, *note));
            if sel.len() as u32 >= n {
                return sel;
            }
        }
    }
    sel
}

/// 选区总音符数。
pub fn total_notes(doc: &MidiDocument) -> usize {
    (0..doc.track_count())
        .map(|t| doc.track_note_count(t as u16) as usize)
        .sum()
}

/// 预计算 origin（复制端两遍扫描的第一遍，不计入复制耗时）。
pub fn compute_origin(sel: &[(usize, NoteEvent)]) -> (u32, u8) {
    let mut min_tick = u32::MAX;
    let mut min_key = u8::MAX;
    for (_, n) in sel {
        if n.start_tick < min_tick {
            min_tick = n.start_tick;
        }
        if n.key < min_key {
            min_key = n.key;
        }
    }
    (min_tick, min_key)
}
