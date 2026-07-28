//! 流式异步提交：直接使用 DragState（BitVec + delta），跳过 MoveOp 中间构造。
//!
//! 与 `async_commit` 的区别：
//! - `async_commit` 接收 `Vec<MoveOp>`（含 original_ticks/keys），用于 undo 精确还原
//! - `async_commit_streaming` 接收 `&DragState`（BitVec + delta），不构造中间 Vec
//!
//! 内存增量：16M 音符全选时仅 BitVec ~2 MB，无 Vec<usize> 或 MoveOp 中间分配。
//! 速度优化：后台线程直接遍历 BitVec，跳过 `selected_indices()` 和 `move_ops_from_drag_state()`。

use super::EditorData;
use super::async_commit::AsyncCommitResult;
use crate::DragState;
use crate::error::{CoreError, Result};
use crate::note::Note;
use bit_vec::BitVec;
use im::Vector;
use std::collections::HashMap;
use std::sync::mpsc;

impl EditorData {
    /// 在后台线程异步应用 DragState（流式，不构造 MoveOp）
    ///
    /// 返回 `true` 表示已启动新后台任务；`false` 表示 delta 为零或未选中。
    /// 同一时刻只允许一个 pending commit。
    pub fn apply_drag_state_async(&mut self, drag_state: &DragState, max_key: u16) -> Result<bool> {
        if drag_state.is_delta_zero() || !drag_state.has_selection() {
            return Ok(false);
        }
        if self.pending_commit.is_some() {
            return Err(CoreError::InvalidArgument(
                "已存在 pending commit，无法启动新的异步提交".to_string(),
            ));
        }

        let notes = self.notes.clone();
        let track_notes = self.track_notes.clone();
        let selected = drag_state.selected.clone(); // BitVec clone = O(N/8) = 2 MB for 16M
        let delta_tick = drag_state.delta_tick;
        let delta_key = drag_state.delta_key;
        let track_id = self.current_track;
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let result = apply_drag_state_to_clones(
                notes,
                track_notes,
                &selected,
                delta_tick,
                delta_key,
                track_id,
                max_key,
            );
            let _ = tx.send(result);
        });

        self.pending_commit = Some(super::async_commit::PendingCommit {
            ops: Vec::new(),
            receiver: rx,
        });
        Ok(true)
    }
}

/// 将 DragState 应用到 notes/track_notes 的克隆副本（流式，不构造 MoveOp）
pub(crate) fn apply_drag_state_to_clones(
    mut notes: Vector<Note>,
    mut track_notes: HashMap<usize, Vector<Note>>,
    selected: &BitVec,
    delta_tick: i64,
    delta_key: i16,
    track_id: usize,
    max_key: u16,
) -> Result<AsyncCommitResult> {
    let total_bits = selected.len();
    let start_time = std::time::Instant::now();
    let dt = delta_tick as f32;
    let dk = delta_key as i32;

    let selected_count = selected.iter().filter(|b| *b).count();
    if selected_count == 0 {
        return Ok(AsyncCommitResult {
            notes,
            track_notes,
            modified: 0,
        });
    }

    if !track_notes.contains_key(&track_id) && !notes.is_empty() {
        track_notes.insert(track_id, notes.clone());
    }

    let mut modified = 0usize;
    let mut processed = 0usize;
    let log_interval = (selected_count / 10).max(1);

    // 直接遍历 BitVec，不构造中间 Vec<usize>
    for (i, is_selected) in selected.iter().enumerate() {
        if !is_selected || i >= notes.len() {
            continue;
        }

        // 修改 notes
        if let Some(note) = notes.get_mut(i) {
            let new_tick = (note.tick + dt).max(0.0);
            let new_key = (note.key as i32 + dk).clamp(0, max_key as i32) as u16;
            if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                note.tick = new_tick;
                note.key = new_key;
                modified += 1;
            }
        }

        // 同步修改 track_notes
        if let Some(track_notes) = track_notes.get_mut(&track_id)
            && let Some(note) = track_notes.get_mut(i)
        {
            let new_tick = (note.tick + dt).max(0.0);
            let new_key = (note.key as i32 + dk).clamp(0, max_key as i32) as u16;
            note.tick = new_tick;
            note.key = new_key;
        }

        processed += 1;
        if processed.is_multiple_of(log_interval) {
            tracing::info!(
                "流式异步提交进度: {}% ({} / {}, 已修改 {})",
                processed * 100 / selected_count,
                processed,
                selected_count,
                modified
            );
        }
    }

    tracing::info!(
        "流式异步提交完成: 修改 {} 个音符, 共扫描 {} bits, 耗时 {:?}",
        modified,
        total_bits,
        start_time.elapsed()
    );

    Ok(AsyncCommitResult {
        notes,
        track_notes,
        modified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_state::DragState;
    use crate::note::Note;
    use bit_vec::BitVec;

    fn make_data_with_notes() -> EditorData {
        let mut data = EditorData::new();
        data.current_track = 1;
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        data.notes.push_back(Note::new(10.0, 62, 1.0));
        data.notes.push_back(Note::new(20.0, 64, 1.0));
        data.track_notes.insert(1, data.notes.clone());
        data
    }

    #[test]
    fn test_streaming_commit_applies_correctly() {
        let mut data = make_data_with_notes();
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        bv.set(2, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(5, -2);

        assert!(data.apply_drag_state_async(&ds, 127).unwrap());
        let modified = loop {
            if let Some(result) = data.poll_async_commit() {
                break result.unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(modified, 2);
        assert_eq!(data.notes[0].tick, 5.0);
        assert_eq!(data.notes[0].key, 58);
        assert_eq!(data.notes[2].tick, 25.0);
        assert_eq!(data.notes[2].key, 62);
    }

    #[test]
    fn test_streaming_commit_zero_delta_is_noop() {
        let mut data = make_data_with_notes();
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        let ds = DragState::new(bv, 0, 60);
        assert!(!data.apply_drag_state_async(&ds, 127).unwrap());
    }

    #[test]
    fn test_streaming_commit_no_selection() {
        let mut data = make_data_with_notes();
        let bv = BitVec::from_elem(3, false);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(5, 0);
        assert!(!data.apply_drag_state_async(&ds, 127).unwrap());
    }

    #[test]
    fn test_streaming_speed_large_scale() {
        // 创建 1600 万音符，测试流式提交性能
        let note_count = 16_000_000;
        let mut data = EditorData::new();
        data.current_track = 1;
        for i in 0..note_count {
            data.notes.push_back(Note::new(i as f32 * 10.0, 60, 5.0));
        }
        data.track_notes.insert(1, data.notes.clone());

        // 选中 50% 的音符（800 万个）
        let mut bv = BitVec::from_elem(note_count, false);
        for i in (0..note_count).step_by(2) {
            bv.set(i, true);
        }
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(10, 3);

        eprintln!(
            "流式提交测试: {} 音符, 50% 选中 ({} 个)",
            note_count,
            note_count / 2
        );
        let start = std::time::Instant::now();

        let result = apply_drag_state_to_clones(
            data.notes.clone(),
            data.track_notes.clone(),
            &ds.selected,
            ds.delta_tick,
            ds.delta_key,
            data.current_track,
            127,
        );

        let elapsed = start.elapsed();
        let result = result.unwrap();
        let rate = if elapsed.as_secs_f64() > 0.0 {
            (result.modified as f64 / elapsed.as_secs_f64()) as u64
        } else {
            0
        };
        eprintln!(
            "流式提交完成: 修改 {} 个音符, 耗时 {:?} ({:.2}M/s)",
            result.modified,
            elapsed,
            rate as f64 / 1_000_000.0
        );

        assert_eq!(result.modified, note_count / 2);
        // 未选中的音符不变
        assert_eq!(result.notes[1].tick, 10.0);
        // 选中的音符已偏移
        assert_eq!(result.notes[0].tick, 10.0);
    }

    #[test]
    fn test_streaming_speed_100_percent_selected() {
        // 1600 万音符全选，测试最坏情况
        let note_count = 16_000_000;
        let mut data = EditorData::new();
        data.current_track = 1;
        for i in 0..note_count {
            data.notes.push_back(Note::new(i as f32 * 10.0, 60, 5.0));
        }
        data.track_notes.insert(1, data.notes.clone());

        let bv = BitVec::from_elem(note_count, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(10, 3);

        eprintln!("流式提交测试（全选）: {} 音符", note_count);
        let start = std::time::Instant::now();

        let result = apply_drag_state_to_clones(
            data.notes.clone(),
            data.track_notes.clone(),
            &ds.selected,
            ds.delta_tick,
            ds.delta_key,
            data.current_track,
            127,
        );

        let elapsed = start.elapsed();
        let result = result.unwrap();
        let rate = if elapsed.as_secs_f64() > 0.0 {
            (result.modified as f64 / elapsed.as_secs_f64()) as u64
        } else {
            0
        };
        eprintln!(
            "流式提交（全选）完成: 修改 {} 个音符, 耗时 {:?} ({:.2}M/s)",
            result.modified,
            elapsed,
            rate as f64 / 1_000_000.0
        );

        assert_eq!(result.modified, note_count);
    }

    #[test]
    fn test_compare_old_vs_new_approach() {
        // 对比新旧两种方案在 1000 万音符下的性能
        let note_count = 10_000_000;
        let mut data = EditorData::new();
        data.current_track = 1;
        for i in 0..note_count {
            data.notes.push_back(Note::new(i as f32 * 10.0, 60, 5.0));
        }
        data.track_notes.insert(1, data.notes.clone());

        // 选中 50% 的音符
        let mut bv = BitVec::from_elem(note_count, false);
        for i in (0..note_count).step_by(2) {
            bv.set(i, true);
        }
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(10, 3);

        // 旧方案：move_ops_from_drag_state + selected_indices()
        eprintln!("\n--- 对比测试: {} 音符, 50% 选中 ---", note_count);
        let start_old = std::time::Instant::now();
        let ops = data.move_ops_from_drag_state(&ds);
        let elapsed_old = start_old.elapsed();
        eprintln!("[旧] move_ops_from_drag_state: {:?}", elapsed_old);
        eprintln!(
            "[旧] MoveOp 数量: {}, 预计内存: {} MB (Vec<usize> + original_ticks/keys)",
            ops.len(),
            (note_count / 2 * (8 + 4 + 2) as usize) / (1024 * 1024)
        );

        // 新方案：直接遍历 BitVec
        let start_new = std::time::Instant::now();
        let result = apply_drag_state_to_clones(
            data.notes.clone(),
            data.track_notes.clone(),
            &ds.selected,
            ds.delta_tick,
            ds.delta_key,
            data.current_track,
            127,
        );
        let elapsed_new = start_new.elapsed();
        let result = result.unwrap();
        eprintln!("[新] 流式提交: {:?}", elapsed_new);
        eprintln!(
            "[新] 修改: {} 音符, 内存增量: ~2 MB (BitVec)",
            result.modified
        );
        eprintln!(
            "[新] 速度提升: {:.1}x (移除了 move_ops_from_drag_state 全量构造开销)",
            elapsed_old.as_secs_f64() / elapsed_new.as_secs_f64().max(1e-9)
        );
    }
}
