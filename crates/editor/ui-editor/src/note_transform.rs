use super::Editor;
use lumino_editor_state::EditorTransform;

use std::collections::HashSet;

impl Editor {
    /// 按速度系数批量改变选中音符的速度（time-stretch）。
    ///
    /// # 参数
    /// * `speed_factor` — 速度变化系数（>1 变快，<1 变慢）
    ///
    /// # 返回
    /// 实际发生变化的音符数量。
    pub fn apply_speed_change(&mut self, speed_factor: f32) -> usize {
        let selected: HashSet<usize> = self.get_selected_indices().into_iter().collect();
        let result = self
            .editor_state
            .data
            .apply_speed_change(&selected, speed_factor);
        if result > 0 {
            self.mark_notes_changed();
            // 2026-09 协作修复：前向变速需广播给对端（变换函数内部已入队）。
            self.broadcast_pending_collab_transform_sync();
        }
        result
    }

    /// 批量编辑选中音符的力度/门时/key/时间属性。
    ///
    /// # 参数
    /// * `velocity` — 力度表达式（字符串编码）
    /// * `gate` — 门时表达式
    /// * `key` — key 表达式
    /// * `tick` — 时间表达式
    /// * `max_key` — 允许的最大 key
    ///
    /// # 返回
    /// 实际发生变化的音符数量。
    pub fn apply_batch_edit(
        &mut self,
        velocity: &str,
        gate: &str,
        key: &str,
        tick: &str,
        max_key: u16,
    ) -> usize {
        let selected: HashSet<usize> = self.get_selected_indices().into_iter().collect();
        let result = self
            .editor_state
            .data
            .apply_batch_edit(&selected, velocity, gate, key, tick, max_key);
        if result > 0 {
            self.mark_notes_changed();
            // 2026-09 协作修复：前向批量编辑（含力度）需广播给对端。
            self.broadcast_pending_collab_transform_sync();
        }
        result
    }
}
