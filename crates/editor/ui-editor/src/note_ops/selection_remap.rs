//! 主选择索引重映射：结构编辑后按音符身份（id）恢复选中
//!
//! 主选择（`selected_notes` / `selection_bitset`）以**当前轨索引**存储。
//! 任何改变当前轨音符列表的操作——远端协作增删移、undo/redo 回放、
//! razor 切割、拖动提交写回、绘制插入——都会位移索引，使选中集指向
//! 错误音符（用户报告：A 端选中一段后 B 端在同轨头部插入音符，
//! A 端再按 Delete 会命中错误音符）。
//!
//! 使用模式（guard，结构编辑前后各一次调用）：
//! ```ignore
//! let identity = editor.capture_selection_identity(); // 结构编辑前
//! // ...mutate（增/删/移/整轨替换）...
//! editor.remap_selection_by_identity(&identity);       // 结构编辑后
//! ```
//!
//! 身份由文档级全局唯一 id 承载（单调分配、删除不回收），经
//! [`ChunkedList::position_of_id`] 精确重定位；未命中的音符（被删/被替换）
//! 自动取消选中。选中集为空时捕获为空快照，重映射为零开销空操作。
//!
//! **维护约束**：新增任何「改变当前轨音符列表」的代码路径时，必须用
//! 本 guard 包裹，否则主选择会再次漂移。
//!
//! [`ChunkedList::position_of_id`]: lumino_midi_model::ChunkedList::position_of_id

use super::Editor;

/// 逐音符捕获上限：超过则放弃捕获（防超大选中集内存/耗时失控），
/// 重映射时保守清空选择（宁可丢选中，不可选错音符）。
const MAX_CAPTURE_ENTRIES: usize = 1 << 18; // 262_144

/// 批量重定位阈值：选中数超过此值时改用单次轨道扫描（O(N)），
/// 避免逐项 `position_of_id` 在「音符已远移」时反复全扫（O(k·N)）。
const BATCH_LOOKUP_THRESHOLD: usize = 256;

/// 结构编辑前捕获的主选择身份快照（见模块文档）
#[derive(Debug)]
pub struct SelectionIdentity {
    /// 捕获时的当前轨：仅该轨的结构编辑会影响选中索引
    track: usize,
    /// 选中音符身份 `(id, start_tick 提示)`；
    /// `None` = 选中集过大未逐音符捕获 → 重映射时保守清空
    entries: Option<Vec<(u64, u32)>>,
}

impl Editor {
    /// 结构编辑前捕获主选择身份（无选中返回空快照，重映射为空操作）
    pub fn capture_selection_identity(&self) -> SelectionIdentity {
        let track = self.editor_state.data.current_track;
        let interaction = &self.editor_state.interaction;
        let notes = self.editor_state.data.track_notes(track);

        // 收集选中索引（bitset 路径当前休眠，但保持语义一致）
        let mut indices: Vec<usize> = Vec::new();
        if let Some(ref bs) = interaction.selection_bitset {
            if bs.count_ones() > MAX_CAPTURE_ENTRIES {
                return SelectionIdentity {
                    track,
                    entries: None,
                };
            }
            bs.for_each_set(|i| indices.push(i));
        } else {
            if interaction.selected_notes.len() > MAX_CAPTURE_ENTRIES {
                return SelectionIdentity {
                    track,
                    entries: None,
                };
            }
            indices.extend(interaction.selected_notes.iter().copied());
        }

        // 转换为稳定身份（id + tick 提示）；id==0（遗留未分配）无法稳定匹配，跳过
        let entries: Vec<(u64, u32)> = indices
            .into_iter()
            .filter_map(|i| notes.get(i))
            .filter(|ev| ev.id != 0)
            .map(|ev| (ev.id, ev.start_tick))
            .collect();
        SelectionIdentity {
            track,
            entries: Some(entries),
        }
    }

    /// 结构编辑后按身份重映射主选择。
    ///
    /// - 捕获轨已非当前轨（选择已被清空/重建）→ 不动；
    /// - 捕获时无选中 → 不动（避免误清空编辑期间新建的选择）；
    /// - 超大选中集未逐音符捕获 → 保守清空；
    /// - 其余：清空后按 id 重定位重建（未命中者取消选中）。
    pub fn remap_selection_by_identity(&mut self, identity: &SelectionIdentity) {
        if identity.track != self.editor_state.data.current_track {
            return;
        }
        let Some(ref entries) = identity.entries else {
            // 超大选中集：保守清空（宁可丢选中，不可选错音符）
            self.selection_clear();
            return;
        };
        if entries.is_empty() {
            return;
        }

        if entries.len() > BATCH_LOOKUP_THRESHOLD {
            self.remap_selection_batch(identity.track, entries);
            return;
        }

        self.selection_clear();
        for &(id, tick_hint) in entries {
            let found = self
                .editor_state
                .data
                .track_notes(identity.track)
                .position_of_id(id, tick_hint);
            if let Some(idx) = found {
                self.selection_insert(idx);
            }
        }
    }

    /// 大批量重定位：单次扫描轨道，id 命中即重选（O(N + k)，不依赖有序提示）
    fn remap_selection_batch(&mut self, track: usize, entries: &[(u64, u32)]) {
        let wanted: std::collections::HashSet<u64> = entries.iter().map(|&(id, _)| id).collect();
        let indices: Vec<usize> = self
            .editor_state
            .data
            .track_notes(track)
            .iter()
            .enumerate()
            .filter_map(|(i, ev)| wanted.contains(&ev.id).then_some(i))
            .collect();
        self.selection_clear();
        for i in indices {
            self.selection_insert(i);
        }
    }
}
