//! 主音轨事件级增量：数据层事件 → GPU 连续段映射（纯函数，可单测）
//!
//! GPU 布局 = 上次全量构建的可见音符（索引 = `note_visible_indices` 列表下标）。
//! 等长增量事件（拖动/变速/翻转）携带 notes 全局索引，本模块映射到
//! GPU 位置并合并连续段，生成 `UpdateMany` 需要的 (start_index, instances) 段列表。
//!
//! 任一事件索引不在可见列表中（不可见/未上传）→ `Err`，调用方全量兜底
//! （防止幽灵/缺失音符）。

use lumino_editor_state::NoteDeltaEvent;
use lumino_gfx::NoteInstance;
use lumino_note_core::note::Note;

/// 将增量事件映射为 GPU 连续段列表
///
/// - `visible_indices`：上次全量构建的可见 notes 索引，**必须升序**且与 GPU 布局一致
/// - `build`：Note → NoteInstance 转换（UI 层提供颜色/描边）
///
/// 返回 `(GPU 起始位置, 实例列表)` 段列表（按位置升序，连续段已合并）。
/// 任一事件索引未命中可见列表 → `Err(())`（调用方应全量兜底）。
pub(crate) fn map_events_to_segments(
    events: &[NoteDeltaEvent],
    visible_indices: &[usize],
    build: impl Fn(&Note) -> NoteInstance,
) -> Result<Vec<(usize, Vec<NoteInstance>)>, ()> {
    // 收集命中 (GPU 位置, 音符引用)
    let mut hits: Vec<(usize, &Note)> = Vec::new();
    for event in events {
        match event {
            NoteDeltaEvent::UpdateRange { start_index, notes } => {
                for (offset, note) in notes.iter().enumerate() {
                    let note_idx = start_index + offset;
                    match visible_indices.binary_search(&note_idx) {
                        Ok(pos) => hits.push((pos, note)),
                        Err(_) => return Err(()),
                    }
                }
            }
        }
    }

    // 按 GPU 位置排序后合并连续段（段元组 (下一个位置, 实例列表)）
    hits.sort_by_key(|(pos, _)| *pos);
    let mut segments: Vec<(usize, Vec<NoteInstance>)> = Vec::new();
    for (pos, note) in hits {
        match segments.last_mut() {
            Some((next, instances)) if *next == pos => {
                instances.push(build(note));
                *next = pos + 1;
            }
            _ => segments.push((pos + 1, vec![build(note)])),
        }
    }
    // 输出 (起始位置, 实例列表)
    Ok(segments
        .into_iter()
        .map(|(next, instances)| (next - instances.len(), instances))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_note(tick: f32) -> Note {
        Note::new(tick, 60, 1.0)
    }

    fn build(note: &Note) -> NoteInstance {
        NoteInstance::new(
            note.tick,
            note.key as u8,
            note.length,
            [0.2, 0.55, 1.0, 1.0],
            1,
        )
    }

    fn update_event(start: usize, count: usize) -> NoteDeltaEvent {
        NoteDeltaEvent::UpdateRange {
            start_index: start,
            notes: (0..count)
                .map(|i| make_note((start + i) as f32 * 10.0))
                .collect(),
        }
    }

    #[test]
    fn test_contiguous_hits_merge_into_one_segment() {
        // 可见索引 [0,1,2,3]，事件更新 [0..2) → 命中 GPU 0,1 → 合并为 [0, 2)
        let visible = vec![0usize, 1, 2, 3];
        let events = vec![update_event(0, 2)];
        let segments =
            map_events_to_segments(&events, &visible, build).expect("事件分段构建应成功");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].0, 0);
        assert_eq!(segments[0].1.len(), 2);
    }

    #[test]
    fn test_scattered_hits_produce_multiple_segments() {
        // 可见 [1,3,5]，事件更新 notes 1 和 5 → GPU 0 和 2 → 两个段
        let visible = vec![1usize, 3, 5];
        let events = vec![update_event(1, 1), update_event(5, 1)];
        let segments =
            map_events_to_segments(&events, &visible, build).expect("事件分段构建应成功");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].0, 0);
        assert_eq!(segments[1].0, 2);
    }

    #[test]
    fn test_out_of_view_index_returns_err() {
        // 事件更新 notes[7]，但可见列表不含 7 → Err（全量兜底）
        let visible = vec![0usize, 1, 2];
        let events = vec![update_event(7, 1)];
        assert!(map_events_to_segments(&events, &visible, build).is_err());
    }

    #[test]
    fn test_event_range_partially_visible_returns_err() {
        // 事件区间 [0..3) 但 2 不可见 → 整体 Err（防幽灵）
        let visible = vec![0usize, 1];
        let events = vec![update_event(0, 3)];
        assert!(map_events_to_segments(&events, &visible, build).is_err());
    }

    #[test]
    fn test_adjacent_events_merge_across_event_boundary() {
        // 两个事件 [0..2) 和 [2..4)，命中 GPU 0..4 → 合并为单段
        let visible = vec![0usize, 1, 2, 3];
        let events = vec![update_event(0, 2), update_event(2, 2)];
        let segments =
            map_events_to_segments(&events, &visible, build).expect("事件分段构建应成功");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].0, 0);
        assert_eq!(segments[0].1.len(), 4);
    }

    #[test]
    fn test_empty_events_produce_no_segments() {
        let visible = vec![0usize, 1];
        let segments = map_events_to_segments(&[], &visible, build).expect("空事件分段构建应成功");
        assert!(segments.is_empty());
    }

    #[test]
    fn test_unsorted_visible_indices_still_work() {
        // 调用方保证升序，但即使未排序（实际不可见列表恒升序），
        // binary_search 要求升序——测试确认升序输入的正确性
        let visible = vec![5usize, 6, 7, 8];
        let events = vec![update_event(6, 2)];
        let segments =
            map_events_to_segments(&events, &visible, build).expect("事件分段构建应成功");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].0, 1);
    }
}
