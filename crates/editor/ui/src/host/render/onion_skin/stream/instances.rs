//! 单音轨 `NoteInstance` 构建（小轨顺序 / 大轨 rayon 分片）。
//!
//! 从 `stream.rs` 拆出：全量会话、轨布局同步、事件级增量三条路径共用同一构建器，
//! 拆出后 `stream.rs` 保持 < 400 行（AGENTS.md 文件长度红线）。

use lumino_editor_state::EditorData;
use lumino_gfx::NoteInstance;

use super::super::{PARALLEL_BUILD_MIN, onion_border_width};

/// 构建单音轨的完整 NoteInstance 列表（分片；数据源：document 单一权威源）
///
/// 2026-08 改造：track_notes 缓存已删除，一律从 `MidiDocument` 读。
/// 无文档 → 空列表。
///
/// 2026-09 并行构建：大轨（≥ [`PARALLEL_BUILD_MIN`]）按索引区间切分，多线程
/// `iter_window` 顺序构建后**按序分片返回**（不做二次拼接拷贝）——单轨 3000W
/// 实例构建实测 ~180ms，是主轨段重建（`TrackDelta`）的主要 CPU 成本。
/// 小轨返回单分片（调用方语义不变）。
pub(super) fn build_track_instance_parts(
    data: &EditorData,
    track_id: usize,
) -> Vec<Vec<NoteInstance>> {
    let color = lumino_extras::palette::current_track_color_f32(track_id);
    // 轨道索引编码进 border_width 高 16 位（稳定深度优先级）
    let border_width = onion_border_width(track_id);

    let Some(doc) = data.document.as_ref() else {
        return Vec::new();
    };
    let doc_notes = doc.track_notes(track_id);
    let total = doc_notes.len();
    if total == 0 {
        return Vec::new();
    }

    if total < PARALLEL_BUILD_MIN {
        let mut v = Vec::with_capacity(total);
        for ne in doc_notes.iter() {
            v.push(NoteInstance::new(
                ne.start_tick as f32,
                ne.key,
                (ne.end_tick - ne.start_tick) as f32,
                color,
                border_width,
            ));
        }
        return vec![v];
    }

    use rayon::prelude::*;
    let workers = rayon::current_num_threads().max(1);
    let part = total.div_ceil(workers);
    (0..workers)
        .into_par_iter()
        .map(|w| {
            let lo = (w * part).min(total);
            let hi = ((w + 1) * part).min(total);
            // 精确预分配：iter_window 无 size_hint，逐元素 push 会触发倍增重分配
            let mut v = Vec::with_capacity(hi.saturating_sub(lo));
            for (_, ne) in doc_notes.iter_window(lo, hi) {
                v.push(NoteInstance::new(
                    ne.start_tick as f32,
                    ne.key,
                    (ne.end_tick - ne.start_tick) as f32,
                    color,
                    border_width,
                ));
            }
            v
        })
        .collect()
}
