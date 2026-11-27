//! 撤销/重做组、删除标记与存活对账。
//!
//! 结构体字段仍在 `drawn_shape.rs`，这里只放方法。

use super::super::*;

impl ShapeSelectState {
    /// 历史分组被撤销：撤销创建 → 隐藏该图形；撤销移动 → 几何反向平移
    ///
    /// **删除状态不在此处理**：删除走的是 Snapshot 历史（撤销时 redo 侧的快照
    /// 不携带分组 ID，无法对称回放），改由 [`Self::reconcile_alive`] 按
    /// 「该图形的音符是否还在文档里」对账，方向无关、天然对称。
    pub fn on_undo_group(&mut self, group: u64) {
        self.replay_group(group, true);
    }

    /// 历史分组被重做：重做创建 → 恢复该图形；重做移动 → 几何正向平移
    pub fn on_redo_group(&mut self, group: u64) {
        self.replay_group(group, false);
    }

    /// 按方向回放分组（`undo = true` 反向，`false` 正向）。
    ///
    /// **删除状态不在此处理**：删除走的是 Snapshot 历史（撤销时 redo 侧的快照
    /// 不携带分组 ID，无法对称回放），改由 [`Self::reconcile_alive`] 按
    /// 「该图形的音符是否还在文档里」对账，方向无关、天然对称。
    fn replay_group(&mut self, group: u64, undo: bool) {
        let sign = if undo { -1.0 } else { 1.0 };
        for s in &mut self.shapes {
            if s.group == Some(group) {
                s.hidden_by_creation = undo;
            }
            // 撤销一次移动 → 几何反向平移回去；重做 → 正向平移
            if let Some(mv) = s.moves.iter().find(|m| m.group == group).copied() {
                let (dtick, dkey) = (mv.dtick * sign, mv.dkey * sign);
                let _ = s.source.translate(dtick, dkey);
                for n in &mut s.notes {
                    *n = n.translated(dtick, dkey);
                }
            }
        }
        self.collapse_selection();
    }

    /// 标记图形为「已删除」（音符已由调用方从文档删除）
    pub fn mark_deleted(&mut self, id: u64) -> bool {
        let Some(s) = self.shapes.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        s.deleted = true;
        s.ever_deleted = true;
        self.collapse_selection();
        true
    }

    /// 与文档对账：按「该图形是否还有音符残留」修正 `deleted`
    ///
    /// `alive` 为 `(图形 ID, 是否还有音符)` 列表（由调用方查文档得出）。
    /// 只对 `ever_deleted` 的图形生效——未被本功能删除过的图形不参与，
    /// 避免误隐藏「用户手动擦除音符但保留轮廓」的场景。
    /// 撤销/重做后调用，使删除状态天然对称（不依赖历史分组 ID）。
    pub fn reconcile_alive(&mut self, alive: &[(u64, bool)]) {
        for (id, ok) in alive {
            if let Some(s) = self.shapes.iter_mut().find(|s| s.id == *id)
                && s.ever_deleted
                && !s.notes.is_empty()
            {
                s.deleted = !*ok;
            }
        }
        self.collapse_selection();
    }

    /// 删除指定图形记录（含选中集收敛）
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.shapes.len();
        self.shapes.retain(|s| s.id != id);
        self.selected.retain(|&x| x != id);
        self.shapes.len() != before
    }

    /// 当前可见图形引用的历史分组集合（供调试 / 测试）
    pub fn active_groups(&self) -> HashSet<u64> {
        self.shapes
            .iter()
            .filter_map(|s| if s.is_visible() { s.group } else { None })
            .collect()
    }
}
