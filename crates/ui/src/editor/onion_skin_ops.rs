//! 洋葱皮简单查询函数（已由瓦片系统替代）
//!
//! 保留 `collect_visible_track_indices` 供 Sidebar/状态查询使用。
//! 音符渲染类函数均返回空，瓦片系统接管后不再使用。

use crate::editor::Editor;
use lumino_gfx::NoteInstance;

#[allow(dead_code)]
impl Editor {
    /// 获取所有洋葱皮音符原始数据 — 已由瓦片系统替代，返回空
    pub fn get_onion_skin_notes(
        &self,
        _track_onion_states: &std::collections::HashMap<usize, bool>,
        _visible_tick_start: f32,
        _visible_tick_end: f32,
        _visible_key_min: u16,
        _visible_key_max: u16,
    ) -> Vec<(f32, u16, f32, iced_core::Color)> {
        Vec::new()
    }

    /// 收集可见音轨索引
    ///
    /// 返回降序排列的音轨索引，确保最后一个音轨渲染在最底层（第一层洋葱皮），
    /// 第一个音轨渲染在最顶层（最后一层洋葱皮），避免闪烁问题。
    pub(super) fn collect_visible_track_indices(
        &self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
    ) -> Vec<usize> {
        let mut indices: Vec<usize> = track_onion_states
            .iter()
            .filter(|(_, is_enabled)| **is_enabled)
            .map(|(&idx, _)| idx)
            .filter(|&idx| idx != self.editor_state.data.current_track)
            .collect();
        indices.sort_by(|a, b| b.cmp(a));
        indices
    }

    /// 收集可见音轨索引（使用缓存）
    pub(super) fn collect_visible_track_indices_cached(
        &mut self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
    ) -> Vec<usize> {
        if self.onion_skin.cache_valid {
            return self.onion_skin.cached_track_indices.clone();
        }

        let indices = self.collect_visible_track_indices(track_onion_states);
        self.onion_skin.cached_track_indices = indices.clone();
        self.onion_skin.cache_valid = true;
        indices
    }

    /// 获取洋葱皮音符实例 — 已由瓦片系统替代，返回空
    pub fn get_onion_skin_instances(
        &mut self,
        _track_idx: usize,
        _track_onion_enabled: bool,
    ) -> Vec<NoteInstance> {
        Vec::new()
    }

    /// 获取所有洋葱皮音符实例 — 已由瓦片系统替代，返回空
    pub fn get_all_onion_skin_instances(
        &mut self,
        _track_onion_states: &std::collections::HashMap<usize, bool>,
    ) -> Vec<NoteInstance> {
        Vec::new()
    }
}
