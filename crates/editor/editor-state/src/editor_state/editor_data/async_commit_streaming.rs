//! 流式异步提交：直接使用 DragState（BitVec + delta），跳过 MoveOp 中间构造。
//!
//! 与 `async_commit` 的区别：
//! - `async_commit` 接收 `Vec<MoveOp>`（含 original_ticks/keys），用于 undo 精确还原
//! - `async_commit_streaming` 接收 `&DragState`（BitVec + delta），不构造中间 Vec
//!
//! 内存增量：16M 音符全选时仅 BitVec ~2 MB，无 Vec<usize> 或 MoveOp 中间分配。
//! 速度优化：后台线程直接遍历 BitVec，跳过 `selected_indices()` 和 `move_ops_from_drag_state()`。
//!
//! 2026-08 单一权威源改造：后台线程克隆当前音轨 `Vec<NoteEvent>`，完成后整轨写回。

use super::EditorData;
use super::async_commit::AsyncCommitResult;
use crate::DragState;
use bit_vec::BitVec;
use lumino_core::error::{CoreError, Result};
use lumino_midi_model::NoteEvent;
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

        let notes = self.current_track_notes().to_vec();
        let selected = drag_state.selected.clone(); // BitVec clone = O(N/8) = 2 MB for 16M
        let delta_tick = drag_state.delta_tick;
        let delta_key = drag_state.delta_key;
        let track_id = self.current_track;
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let drag_result = apply_drag_state_to_clones(
                notes, &selected, delta_tick, delta_key, track_id, max_key,
            );
            let _ = tx.send(drag_result);
        });

        self.pending_commit = Some(super::async_commit::PendingCommit {
            ops: Vec::new(),
            receiver: rx,
        });
        Ok(true)
    }
}

/// 将 DragState 应用到当前音轨音符的克隆副本（流式，不构造 MoveOp）
pub(crate) fn apply_drag_state_to_clones(
    mut notes: Vec<NoteEvent>,
    selected: &BitVec,
    delta_tick: i64,
    delta_key: i16,
    _track_id: usize,
    max_key: u16,
) -> Result<AsyncCommitResult> {
    let total_bits = selected.len();
    let start_time = std::time::Instant::now();
    let dt = delta_tick as i32;
    let dk = delta_key as i32;

    let selected_count = selected.iter().filter(|&selected| selected).count();
    if selected_count == 0 {
        return Ok(AsyncCommitResult { notes, modified: 0 });
    }

    let mut modified = 0usize;
    let mut processed = 0usize;
    let log_interval = (selected_count / 10).max(1);

    // 直接遍历 BitVec，不构造中间 Vec<usize>
    for (i, is_selected) in selected.iter().enumerate() {
        if !is_selected || i >= notes.len() {
            continue;
        }

        if let Some(note) = notes.get_mut(i) {
            let new_tick = (note.start_tick as i64 + dt as i64).max(0) as u32;
            let new_key = (note.key as i32 + dk).clamp(0, max_key as i32) as u8;
            if note.start_tick != new_tick || note.key != new_key {
                note.start_tick = new_tick;
                note.end_tick = note.end_tick.max(new_tick.saturating_add(1));
                note.key = new_key;
                modified += 1;
            }
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

    Ok(AsyncCommitResult { notes, modified })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_state::DragState;
    use bit_vec::BitVec;
    use lumino_note_core::note::Note;

    fn make_data_with_notes() -> EditorData {
        EditorData::with_f32_notes(
            1,
            &[
                Note::new(0.0, 60, 1.0),
                Note::new(10.0, 62, 1.0),
                Note::new(20.0, 64, 1.0),
            ],
        )
    }

    #[test]
    fn test_streaming_commit_applies_correctly() {
        let mut editor_data = make_data_with_notes();
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        bv.set(2, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(5, -2);

        assert!(
            editor_data
                .apply_drag_state_async(&ds, 127)
                .expect("异步拖拽提交应成功")
        );
        let modified = loop {
            if let Some(result) = editor_data.poll_async_commit() {
                break result.expect("异步提交应成功");
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(modified, 2);
        assert_eq!(
            editor_data
                .get_note_view(0)
                .expect("第 1 个音符视图应存在")
                .tick,
            5.0
        );
        assert_eq!(
            editor_data
                .get_note_view(0)
                .expect("第 1 个音符视图应存在")
                .key,
            58
        );
        assert_eq!(
            editor_data
                .get_note_view(2)
                .expect("第 3 个音符视图应存在")
                .tick,
            25.0
        );
        assert_eq!(
            editor_data
                .get_note_view(2)
                .expect("第 3 个音符视图应存在")
                .key,
            62
        );
    }

    #[test]
    fn test_streaming_commit_zero_delta_is_noop() {
        let mut editor_data = make_data_with_notes();
        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        let ds = DragState::new(bv, 0, 60);
        assert!(
            !editor_data
                .apply_drag_state_async(&ds, 127)
                .expect("异步拖拽提交应成功")
        );
    }

    #[test]
    fn test_streaming_commit_no_selection() {
        let mut editor_data = make_data_with_notes();
        let bv = BitVec::from_elem(3, false);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(5, 0);
        assert!(
            !editor_data
                .apply_drag_state_async(&ds, 127)
                .expect("异步拖拽提交应成功")
        );
    }

    #[test]
    fn test_streaming_speed_large_scale() {
        // 创建 1600 万音符，测试流式提交性能。
        // 2026-08：直接构造 Vec<NoteEvent> 克隆副本（不经 EditorData/document），
        // 与 apply_drag_state_to_clones 的输入同构，避免 1600 万次 document 插入开销。
        let note_count = 16_000_000;
        let notes: Vec<NoteEvent> = (0..note_count)
            .map(|i| NoteEvent::new((i * 10) as u32, (i * 10 + 5) as u32, 60, 127, 0))
            .collect();

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

        let drag_result =
            apply_drag_state_to_clones(notes, &ds.selected, ds.delta_tick, ds.delta_key, 1, 127);

        let elapsed = start.elapsed();
        let drag_result = drag_result.expect("异步拖拽结果应成功");
        let rate = if elapsed.as_secs_f64() > 0.0 {
            (drag_result.modified as f64 / elapsed.as_secs_f64()) as u64
        } else {
            0
        };
        eprintln!(
            "流式提交完成: 修改 {} 个音符, 耗时 {:?} ({:.2}M/s)",
            drag_result.modified,
            elapsed,
            rate as f64 / 1_000_000.0
        );

        assert_eq!(drag_result.modified, note_count / 2);
        // 未选中的音符不变
        assert_eq!(drag_result.notes[1].start_tick, 10);
        // 选中的音符已偏移
        assert_eq!(drag_result.notes[0].start_tick, 10);
    }

    #[test]
    fn test_streaming_speed_100_percent_selected() {
        // 1600 万音符全选，测试最坏情况
        let note_count = 16_000_000;
        let notes: Vec<NoteEvent> = (0..note_count)
            .map(|i| NoteEvent::new((i * 10) as u32, (i * 10 + 5) as u32, 60, 127, 0))
            .collect();

        let bv = BitVec::from_elem(note_count, true);
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(10, 3);

        eprintln!("流式提交测试（全选）: {} 音符", note_count);
        let start = std::time::Instant::now();

        let drag_result =
            apply_drag_state_to_clones(notes, &ds.selected, ds.delta_tick, ds.delta_key, 1, 127);

        let elapsed = start.elapsed();
        let drag_result = drag_result.expect("异步拖拽结果应成功");
        let rate = if elapsed.as_secs_f64() > 0.0 {
            (drag_result.modified as f64 / elapsed.as_secs_f64()) as u64
        } else {
            0
        };
        eprintln!(
            "流式提交（全选）完成: 修改 {} 个音符, 耗时 {:?} ({:.2}M/s)",
            drag_result.modified,
            elapsed,
            rate as f64 / 1_000_000.0
        );

        assert_eq!(drag_result.modified, note_count);
    }

    #[test]
    fn test_compare_old_vs_new_approach() {
        // 对比新旧两种方案在 1000 万音符下的性能
        let note_count = 10_000_000;
        let notes: Vec<NoteEvent> = (0..note_count)
            .map(|i| NoteEvent::new((i * 10) as u32, (i * 10 + 5) as u32, 60, 127, 0))
            .collect();

        // 选中 50% 的音符
        let mut bv = BitVec::from_elem(note_count, false);
        for i in (0..note_count).step_by(2) {
            bv.set(i, true);
        }
        let mut ds = DragState::new(bv, 0, 60);
        ds.set_delta(10, 3);

        // 旧方案：move_ops_from_drag_state + selected_indices()
        // 对比的是构造开销，EditorData 无需持有音符数据（original_ticks 提取
        // 仅影响 inverse 还原，不参与本次耗时对比的主体路径）
        eprintln!("\n--- 对比测试: {} 音符, 50% 选中 ---", note_count);
        let editor_data = EditorData::new();
        let start_old = std::time::Instant::now();
        let ops = editor_data.move_ops_from_drag_state(&ds);
        let elapsed_old = start_old.elapsed();
        eprintln!("[旧] move_ops_from_drag_state: {:?}", elapsed_old);
        eprintln!(
            "[旧] MoveOp 数量: {}, 预计内存: {} MB (Vec<usize> + original_ticks/keys)",
            ops.len(),
            (note_count / 2 * (8 + 4 + 2) as usize) / (1024 * 1024)
        );

        // 新方案：直接遍历 BitVec
        let start_new = std::time::Instant::now();
        let drag_result =
            apply_drag_state_to_clones(notes, &ds.selected, ds.delta_tick, ds.delta_key, 1, 127);
        let elapsed_new = start_new.elapsed();
        let drag_result = drag_result.expect("异步拖拽结果应成功");
        eprintln!("[新] 流式提交: {:?}", elapsed_new);
        eprintln!(
            "[新] 修改: {} 音符, 内存增量: ~2 MB (BitVec)",
            drag_result.modified
        );
        eprintln!(
            "[新] 速度提升: {:.1}x (移除了 move_ops_from_drag_state 全量构造开销)",
            elapsed_old.as_secs_f64() / elapsed_new.as_secs_f64().max(1e-9)
        );
    }
}
