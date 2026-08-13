//! NoteStore 单元测试

use super::super::note_store::BitSet;
use super::{CHUNK_SIZE, Chunk, NoteStore};
use crate::note::Note;

fn make_test_store(note_count: usize) -> NoteStore {
    let mut store = NoteStore::new();
    for i in 0..note_count {
        store.push_back(Note::new(i as f32 * 10.0, 60 + (i % 24) as u16, 5.0));
    }
    store
}

#[test]
fn test_push_back_and_get() {
    let mut store = NoteStore::new();
    store.push_back(Note::new(100.0, 60, 480.0));
    store.push_back(Note::new(200.0, 62, 240.0));

    assert_eq!(store.len(), 2);
    assert!(!store.is_empty());

    let note0 = store.get(0).expect("第 1 个音符应存在");
    assert_eq!(note0.tick, 100.0);
    assert_eq!(note0.key, 60);

    let note1 = store.get(1).expect("第 2 个音符应存在");
    assert_eq!(note1.tick, 200.0);
    assert_eq!(note1.key, 62);
}

#[test]
fn test_cross_chunk_boundary() {
    // 推入 CHUNK_SIZE + 10 个音符，测试跨块
    let mut store = NoteStore::new();
    for i in 0..CHUNK_SIZE + 10 {
        store.push_back(Note::new(i as f32, 60, 1.0));
    }
    assert_eq!(store.len(), CHUNK_SIZE + 10);

    // 检查跨块边界
    let n_last_in_chunk = store.get(CHUNK_SIZE - 1).expect("块尾音符应存在");
    assert_eq!(n_last_in_chunk.tick, (CHUNK_SIZE - 1) as f32);

    let n_first_in_next = store.get(CHUNK_SIZE).expect("下一块首音符应存在");
    assert_eq!(n_first_in_next.tick, CHUNK_SIZE as f32);

    let n_last = store.get(CHUNK_SIZE + 9).expect("跨块音符应存在");
    assert_eq!(n_last.tick, (CHUNK_SIZE + 9) as f32);
}

#[test]
fn test_iter() {
    let store = make_test_store(5);
    let notes: Vec<Note> = store.iter().collect();
    assert_eq!(notes.len(), 5);
    assert_eq!(notes[0].tick, 0.0);
    assert_eq!(notes[4].tick, 40.0);
}

#[test]
fn test_iter_refs() {
    let store = make_test_store(5);
    let views: Vec<_> = store.iter_refs().collect();
    assert_eq!(views.len(), 5);
    assert_eq!(views[0].tick, 0.0);
    assert_eq!(views[4].tick, 40.0);
}

#[test]
fn test_for_each_ref() {
    let store = make_test_store(5);
    let mut collected = Vec::new();
    store.for_each_ref(|idx, view| collected.push((idx, view.tick)));
    assert_eq!(collected.len(), 5);
    assert_eq!(collected[0], (0, 0.0));
    assert_eq!(collected[4], (4, 40.0));
}

#[test]
fn test_modify() {
    let mut store = make_test_store(3);
    let modified = store.modify(1, |note| {
        note.tick = 999.0;
        note.key = 100;
    });
    assert!(modified);
    assert_eq!(store.get(1).expect("第 2 个音符应存在").tick, 999.0);
    assert_eq!(store.get(1).expect("第 2 个音符应存在").key, 100);
}

#[test]
fn test_get_mut() {
    let mut store = make_test_store(3);
    {
        let mut nm = store.get_mut(1).expect("第 2 个音符应存在");
        nm.set_tick(500.0);
        nm.set_key(80);
    }
    assert_eq!(store.get(1).expect("第 2 个音符应存在").tick, 500.0);
    assert_eq!(store.get(1).expect("第 2 个音符应存在").key, 80);
}

#[test]
fn test_remove() {
    let mut store = make_test_store(5);
    let removed = store.remove(2).expect("第 3 个音符应存在");
    assert_eq!(removed.tick, 20.0);

    assert_eq!(store.len(), 4);
    // 后续元素前移
    assert_eq!(store.get(0).expect("第 1 个音符应存在").tick, 0.0);
    assert_eq!(store.get(1).expect("第 2 个音符应存在").tick, 10.0);
    assert_eq!(store.get(2).expect("第 3 个音符应存在").tick, 30.0);
    assert_eq!(store.get(3).expect("第 4 个音符应存在").tick, 40.0);
}

#[test]
fn test_insert() {
    let mut store = make_test_store(3);
    store.insert(1, Note::new(500.0, 70, 2.0));

    assert_eq!(store.len(), 4);
    assert_eq!(store.get(0).expect("第 1 个音符应存在").tick, 0.0);
    assert_eq!(store.get(1).expect("第 2 个音符应存在").tick, 500.0);
    assert_eq!(store.get(2).expect("第 3 个音符应存在").tick, 10.0);
    assert_eq!(store.get(3).expect("第 4 个音符应存在").tick, 20.0);
}

#[test]
fn test_retain() {
    let mut store = make_test_store(10);
    store.retain(|note| note.tick < 50.0);

    assert_eq!(store.len(), 5);
    assert_eq!(store.get(0).expect("第 1 个音符应存在").tick, 0.0);
    assert_eq!(store.get(4).expect("第 5 个音符应存在").tick, 40.0);
}

#[test]
fn test_clear() {
    let mut store = make_test_store(5);
    store.clear();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

#[test]
fn test_batch_move_parallel() {
    let mut store = make_test_store(1000);
    let mut sel = BitSet::new(1000);
    for i in (0..1000).step_by(2) {
        sel.set(i);
    }

    let modified = store.batch_move_parallel(&sel, 10.0, 3, 127);
    assert_eq!(modified, 500);

    // 检查选中音符已移动
    assert_eq!(store.get(0).expect("第 1 个音符应存在").tick, 10.0);
    assert_eq!(store.get(0).expect("第 1 个音符应存在").key, 63);
    // 未选中音符不变
    assert_eq!(store.get(1).expect("第 2 个音符应存在").tick, 10.0);
    assert_eq!(store.get(1).expect("第 2 个音符应存在").key, 61);
}

#[test]
fn test_delete_indices() {
    let mut store = make_test_store(10);
    let deleted = store.delete_indices(&[2, 5, 8]);
    assert_eq!(deleted, 3);
    assert_eq!(store.len(), 7);
    // 保留: 0,1,3,4,6,7,9
    assert_eq!(store.get(0).expect("第 1 个音符应存在").tick, 0.0);
    assert_eq!(store.get(1).expect("第 2 个音符应存在").tick, 10.0);
    assert_eq!(store.get(2).expect("第 3 个音符应存在").tick, 30.0);
    assert_eq!(store.get(3).expect("第 4 个音符应存在").tick, 40.0);
    assert_eq!(store.get(4).expect("第 5 个音符应存在").tick, 60.0);
    assert_eq!(store.get(5).expect("第 6 个音符应存在").tick, 70.0);
    assert_eq!(store.get(6).expect("第 7 个音符应存在").tick, 90.0);
}

#[test]
fn test_from_to_im_vector() {
    let mut notes_im = im::Vector::new();
    notes_im.push_back(Note::new(1.0, 60, 10.0));
    notes_im.push_back(Note::new(2.0, 62, 20.0));

    let store = NoteStore::from_im_vector(&notes_im);
    assert_eq!(store.len(), 2);
    assert_eq!(store.get(0).expect("第 1 个音符应存在").tick, 1.0);

    let v2 = store.to_im_vector();
    assert_eq!(v2.len(), 2);
    assert_eq!(v2[0].tick, 1.0);
    assert_eq!(v2[1].tick, 2.0);
}

#[test]
fn test_clone() {
    let mut store = make_test_store(5);
    let s2 = store.clone();
    assert_eq!(s2.len(), 5);

    // 修改原存储不影响克隆
    store.modify(0, |note| note.tick = 999.0);
    assert_eq!(store.get(0).expect("第 1 个音符应存在").tick, 999.0);
    assert_eq!(s2.get(0).expect("克隆存储第 1 个音符应存在").tick, 0.0);
}

#[test]
fn test_large_scale_batch_move() {
    // 10 万音符批量移动性能测试
    let count = 100_000;
    let mut store = make_test_store(count);
    let mut sel = BitSet::new(count);
    for i in (0..count).step_by(2) {
        sel.set(i);
    }

    let start = std::time::Instant::now();
    let modified = store.batch_move_parallel(&sel, 10.0, 3, 127);
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
    let store = make_test_store(100_000);
    let mem_mb = store.memory_mb();
    // 100K 音符 × 12 bytes = 1.2 MB 数据 + 少量开销
    assert!(
        mem_mb > 1.0 && mem_mb < 3.0,
        "内存应在 1-3 MB 之间, 实际: {}",
        mem_mb
    );
}

#[test]
fn test_chunk_remaining() {
    let mut c = Chunk::new();
    assert_eq!(c.remaining(), CHUNK_SIZE);
    c.push(&Note::new(0.0, 60, 1.0));
    assert_eq!(c.remaining(), CHUNK_SIZE - 1);
}
