//! 音符操作子模块入口
//!
//! 按职责拆分以满足 ≤400 行约束：
//! - `selection`: 选中集合增删改（insert/remove/clear/assign）
//! - `selection_remap`: 结构编辑后主选择索引重映射（按 id 恢复选中）
//! - `hit_test`: 音符命中检测（hit_test_note + note_hit_type）
//! - `delete`: 音符删除（delete_note_by_index/delete_note_at/delete_selected_notes）
//! - `selection_box`: 选择框边界计算（get_selection_box_bounds + hit_test_selection_box）

mod delete;
mod hit_test;
mod selection;
mod selection_box;
pub(crate) mod selection_remap;

use super::Editor;

impl Editor {
    /// 指定索引的音符是否被选中。
    ///
    /// # 参数
    /// * `index` — 待检测的音符索引
    ///
    /// # 返回
    /// 命中选中集合返回 `true`。
    pub fn is_note_selected(&self, index: usize) -> bool {
        self.editor_state
            .interaction
            .selected_notes
            .contains(&index)
    }

    /// 当前选中的音符数量。
    ///
    /// # 返回
    /// 选中音符个数。
    pub fn selected_notes_count(&self) -> usize {
        self.editor_state.interaction.selected_notes.len()
    }

    /// 是否有任何选中音符
    pub fn has_selection(&self) -> bool {
        !self.editor_state.interaction.selected_notes.is_empty()
    }

    /// 获取选中索引列表
    pub fn get_selected_indices(&self) -> Vec<usize> {
        self.editor_state
            .interaction
            .selected_notes
            .iter()
            .copied()
            .collect()
    }

    /// 清空当前选中集合。
    pub fn clear_selection(&mut self) {
        self.selection_clear();
    }

    /// 选中当前 track 的全部音符。
    ///
    /// 2026-09 性能修复：不再「构建新 HashSet → 整体替换」（百万级选中每轮重建
    /// 36MB 哈希表 + 首触缺页），改为**原地 clear + reserve + 顺序插入**复用既有
    /// 容量；边界缓存由一次轨道顺序扫描得出（key 非有序，无法 O(1)）。
    /// 边界扫描与索引插入分两遍（顺序读 + 顺序写分离，实测快于交错单遍）。
    pub fn select_all_notes(&mut self) {
        let data = &self.editor_state.data;
        let note_count = data.current_track_note_count();

        // 边界：一次顺序扫描（全选时无需哈希命中判断）
        let mut min_t = f32::INFINITY;
        let mut max_te = f32::NEG_INFINITY;
        let mut max_k = u16::MIN;
        let mut min_k = u16::MAX;
        if note_count > 0 {
            for n in data.current_track_notes().iter() {
                let tick = n.start_tick as f32;
                let length = (n.end_tick - n.start_tick) as f32;
                min_t = min_t.min(tick);
                max_te = max_te.max(tick + length);
                max_k = max_k.max(n.key as u16);
                min_k = min_k.min(n.key as u16);
            }
        }

        let set = &mut self.editor_state.interaction.selected_notes;
        set.clear();
        set.reserve(note_count);
        for i in 0..note_count {
            set.insert(i);
        }
        self.selected_bounds.set(if note_count > 0 {
            Some((min_t, max_te, max_k, min_k))
        } else {
            None
        });
    }
}
