//! NoteStore 单元测试

#![allow(clippy::unwrap_used)]

use super::super::note_store::BitSet;
use super::{CHUNK_SIZE, Chunk, NoteStore};
use crate::note::Note;

fn make_notes(count: usize) -> NoteStore {
    let mut s = NoteStore::new();
    for i in 0..count {
        s.push_back(Note::new(i as f32 * 10.0, 60 + (i % 24) as u16, 5.0));
    }
    s
}

#[test]
fn test_push_back_and_get() {
    let mut s = NoteStore::new();
    s.push_back(Note::new(100.0, 60, 480.0));
    s.push_back(Note::new(200.0, 62, 240.0));

    assert_eq!(s.len(), 2);
    assert!(!s.is_empty());

    let n0 = s.get(0).unwrap();
    assert_eq!(n0.tick, 100.0);
    assert_eq!(n0.key, 60);

    let n1 = s.get(1).unwrap();
    assert_eq!(n1.tick, 200.0);
    assert_eq!(n1.key, 62);
}

#[test]
fn test_cross_chunk_boundary() {
    // 推入 CHUNK_SIZE + 10 个音符，测试跨块
    let mut s = NoteStore::new();
    for i in 0..CHUNK_SIZE + 10 {
        s.push_back(Note::new(i as f32, 60, 1.0));
    }
    assert_eq!(s.len(), CHUNK_SIZE + 10);

    // 检查跨块边界
    let n_last_in_chunk = s.get(CHUNK_SIZE - 1).unwrap();
    assert_eq!(n_last_in_chunk.tick, (CHUNK_SIZE - 1) as f32);

    let n_first_in_next = s.get(CHUNK_SIZE).unwrap();
    assert_eq!(n_first_in_next.tick, CHUNK_SIZE as f32);

    let n_last = s.get(CHUNK_SIZE + 9).unwrap();
    assert_eq!(n_last.tick, (CHUNK_SIZE + 9) as f32);
}

#[test]
fn test_iter() {
    let s = make_notes(5);
    let notes: Vec<Note> = s.iter().collect();
    assert_eq!(notes.len(), 5);
    assert_eq!(notes[0].tick, 0.0);
    assert_eq!(notes[4].tick, 40.0);
}

#[test]
fn test_iter_refs() {
    let s = make_notes(5);
    let views: Vec<_> = s.iter_refs().collect();
    assert_eq!(views.len(), 5);
    assert_eq!(views[0].tick, 0.0);
    assert_eq!(views[4].tick, 40.0);
}

#[test]
fn test_for_each_ref() {
    let s = make_notes(5);
    let mut collected = Vec::new();
    s.for_each_ref(|idx, view| collected.push((idx, view.tick)));
    assert_eq!(collected.len(), 5);
    assert_eq!(collected[0], (0, 0.0));
    assert_eq!(collected[4], (4, 40.0));
}

#[test]
fn test_modify() {
    let mut s = make_notes(3);
    let modified = s.modify(1, |n| {
        n.tick = 999.0;
        n.key = 100;
    });
    assert!(modified);
    assert_eq!(s.get(1).unwrap().tick, 999.0);
    assert_eq!(s.get(1).unwrap().key, 100);
}

#[test]
fn test_get_mut() {
    let mut s = make_notes(3);
    {
        let mut nm = s.get_mut(1).unwrap();
        nm.set_tick(500.0);
        nm.set_key(80);
    }
    assert_eq!(s.get(1).unwrap().tick, 500.0);
    assert_eq!(s.get(1).unwrap().key, 80);
}

#[test]
fn test_remove() {
    let mut s = make_notes(5);
    let removed = s.remove(2).unwrap();
    assert_eq!(removed.tick, 20.0);

    assert_eq!(s.len(), 4);
    // 后续元素前移
    assert_eq!(s.get(0).unwrap().tick, 0.0);
    assert_eq!(s.get(1).unwrap().tick, 10.0);
    assert_eq!(s.get(2).unwrap().tick, 30.0);
    assert_eq!(s.get(3).unwrap().tick, 40.0);
}

#[test]
fn test_insert() {
    let mut s = make_notes(3);
    s.insert(1, Note::new(500.0, 70, 2.0));

    assert_eq!(s.len(), 4);
    assert_eq!(s.get(0).unwrap().tick, 0.0);
    assert_eq!(s.get(1).unwrap().tick, 500.0);
    assert_eq!(s.get(2).unwrap().tick, 10.0);
    assert_eq!(s.get(3).unwrap().tick, 20.0);
}

#[test]
fn test_retain() {
    let mut s = make_notes(10);
    s.retain(|n| n.tick < 50.0);

    assert_eq!(s.len(), 5);
    assert_eq!(s.get(0).unwrap().tick, 0.0);
    assert_eq!(s.get(4).unwrap().tick, 40.0);
}

#[test]
fn test_clear() {
    let mut s = make_notes(5);
    s.clear();
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);
}

#[test]
fn test_batch_move_parallel() {
    let mut s = make_notes(1000);
    let mut sel = BitSet::new(1000);
    for i in (0..1000).step_by(2) {
        sel.set(i);
    }

    let modified = s.batch_move_parallel(&sel, 10.0, 3, 127);
    assert_eq!(modified, 500);

    // 检查选中音符已移动
    assert_eq!(s.get(0).unwrap().tick, 10.0);
    assert_eq!(s.get(0).unwrap().key, 63);
    // 未选中音符不变
    assert_eq!(s.get(1).unwrap().tick, 10.0);
    assert_eq!(s.get(1).unwrap().key, 61);
}

#[test]
fn test_delete_indices() {
    let mut s = make_notes(10);
    let deleted = s.delete_indices(&[2, 5, 8]);
    assert_eq!(deleted, 3);
    assert_eq!(s.len(), 7);
    // 保留: 0,1,3,4,6,7,9
    assert_eq!(s.get(0).unwrap().tick, 0.0);
    assert_eq!(s.get(1).unwrap().tick, 10.0);
    assert_eq!(s.get(2).unwrap().tick, 30.0);
    assert_eq!(s.get(3).unwrap().tick, 40.0);
    assert_eq!(s.get(4).unwrap().tick, 60.0);
    assert_eq!(s.get(5).unwrap().tick, 70.0);
    assert_eq!(s.get(6).unwrap().tick, 90.0);
}

#[test]
fn test_from_to_im_vector() {
    let mut v = im::Vector::new();
    v.push_back(Note::new(1.0, 60, 10.0));
    v.push_back(Note::new(2.0, 62, 20.0));

    let s = NoteStore::from_im_vector(&v);
    assert_eq!(s.len(), 2);
    assert_eq!(s.get(0).unwrap().tick, 1.0);

    let v2 = s.to_im_vector();
    assert_eq!(v2.len(), 2);
    assert_eq!(v2[0].tick, 1.0);
    assert_eq!(v2[1].tick, 2.0);
}

#[test]
fn test_clone() {
    let mut s = make_notes(5);
    let s2 = s.clone();
    assert_eq!(s2.len(), 5);

    // 修改原存储不影响克隆
    s.modify(0, |n| n.tick = 999.0);
    assert_eq!(s.get(0).unwrap().tick, 999.0);
    assert_eq!(s2.get(0).unwrap().tick, 0.0);
}

#[test]
fn test_large_scale_batch_move() {
    // 10 万音符批量移动性能测试
    let count = 100_000;
    let mut s = make_notes(count);
    let mut sel = BitSet::new(count);
    for i in (0..count).step_by(2) {
        sel.set(i);
    }

    let start = std::time::Instant::now();
    let modified = s.batch_move_parallel(&sel, 10.0, 3, 127);
    let elapsed = start.elapsed();

    assert_eq!(modified, count / 2);
    eprintln!(
        "批量移动 {} 音符 (50% 选中): {:?} ({:.1}M/s)",
        count,
        elapsed,
        (count as f64) / elapsed.as_secs_f64() / 1_000_000.0
    );
}

#[test]
fn test_memory_mb() {
    let s = make_notes(100_000);
    let mb = s.memory_mb();
    // 100K 音符 × 12 bytes = 1.2 MB 数据 + 少量开销
    assert!(mb > 1.0 && mb < 3.0, "内存应在 1-3 MB 之间, 实际: {}", mb);
}

#[test]
fn test_chunk_remaining() {
    let mut c = Chunk::new();
    assert_eq!(c.remaining(), CHUNK_SIZE);
    c.push(&Note::new(0.0, 60, 1.0));
    assert_eq!(c.remaining(), CHUNK_SIZE - 1);
}
