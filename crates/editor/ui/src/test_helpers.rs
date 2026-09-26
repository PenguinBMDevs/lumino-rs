//! 测试共享辅助（仅 `cfg(test)` 编译）。
//!
//! 收敛散落在 handlers_tests / root_tests / midi tests 中的
//! 25 字段 `MidiDocument` 字面量重复构造——改动字段只需改这一处。

use std::sync::{Mutex, MutexGuard};

/// 串行化「全局事件队列」相关测试。
///
/// `crate::event` 的队列是进程级全局状态，而同一测试二进制内的测试并行执行：
/// 一个测试的 `take_events()` 会偷走另一个测试刚发出的事件（CI Ubuntu 实测
/// `test_dialog_handler_opens_custom_precision` 因事件被并发测试取走而闪断）。
/// 所有访问该队列的测试需在开头获取此锁。
pub fn event_queue_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 构造最小 2 轨 MidiDocument（音符写入 document，单一权威源）。
pub fn make_test_document() -> lumino_midi_loader::MidiDocument {
    lumino_midi_loader::MidiDocument {
        notes: vec![
            lumino_midi_loader::ChunkedList::new(),
            lumino_midi_loader::ChunkedList::new(),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Track 0".into()), Some("Track 1".into())],
        total_ticks: 0,
        track_count: 2,
        tracks: lumino_midi_loader::TrackManager::new(2),
        division: 480,
        track_ports: vec![0, 0],
        track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(2),
    }
}
