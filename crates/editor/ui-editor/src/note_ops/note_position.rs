//! 音符按值定位原语：同 tick 段内跳过已占用索引的定位
//!
//! 本原语是「重选/重映射」三条链路的唯一共享实现：
//! - 异步提交重选（`impls/editor_impl/commit.rs`）
//! - 结构编辑后主选择重映射（`note_ops/selection_remap.rs`）
//! - 撤销/重做后按目标值重选（`impls/editor_impl/history.rs`）
//!
//! **为什么必须唯一**：同值多份（stacked 重复音符）场景下，若不做「已占用索引跳过」，
//! 多份同值会全部选中同一索引——选中丢份、撤销丢份，且**静默无提示**。
//! 该原语的正确性直接决定同值多份场景的选中/撤销完整性，复制三份即三处潜在静默错误；
//! 故收敛为单点实现，改一处即全局生效。

/// 同 tick 段内跳过已占用索引的按值定位（同值多份按份数分配）。
///
/// 先 `partition_point` 到同 tick 段首，再线性扫描同 tick 段内全字段匹配且未被占用的首个索引；
/// 未命中返回 None（无全扫兜底）。调用方负责将返回索引插入 `used`。
pub(crate) fn position_of_unused(
    data: &lumino_editor_state::EditorData,
    track: usize,
    target: &lumino_midi_model::NoteEvent,
    used: &std::collections::HashSet<usize>,
) -> Option<usize> {
    let track_notes = data.track_notes(track);
    let start = track_notes.partition_point(target.start_tick);
    let len = track_notes.len();
    let mut i = start;
    while i < len {
        let Some(n) = track_notes.get(i) else {
            break;
        };
        if n.start_tick != target.start_tick {
            break;
        }
        if n == target && !used.contains(&i) {
            return Some(i);
        }
        i += 1;
    }
    None
}
