use iced_core::Point;

use crate::constants::editor::NOTE_EDGE_THRESHOLD_PX;

use super::{Editor, HitType};

impl Editor {
    /// 碰撞检测：测试点击位置是否命中音符
    pub fn hit_test_note(&self, pos: Point) -> Option<(usize, HitType)> {
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);

        for (i, note) in self.notes.iter().enumerate().rev() {
            if note.key == key && tick >= note.tick && tick <= note.tick + note.length {
                let start_dist = (tick - note.tick).abs();
                let end_dist = (tick - (note.tick + note.length)).abs();
                let edge_threshold = NOTE_EDGE_THRESHOLD_PX / self.state.zoom_x;

                if end_dist < edge_threshold {
                    return Some((i, HitType::End));
                } else if start_dist < edge_threshold {
                    return Some((i, HitType::Start));
                } else {
                    return Some((i, HitType::Middle));
                }
            }
        }
        None
    }

    /// 删除指定索引的音符
    ///
    /// # Arguments
    /// * `index` - 音符在 notes 列表中的索引
    pub fn delete_note_by_index(&mut self, index: usize) {
        if index < self.notes.len() {
            let note = self.notes.remove(index);
            tracing::debug!(
                "Editor: deleted note at index {} (tick={}, key={})",
                index,
                note.tick,
                note.key
            );

            // 更新当前音轨的存储
            if !self.notes.is_empty() {
                self.track_notes
                    .insert(self.current_track, self.notes.clone());
            } else {
                // 如果音符列表为空，从 track_notes 中移除该音轨
                self.track_notes.remove(&self.current_track);
            }

            // 清除悬停状态（如果被删除的音符正好是悬停的）
            if let Some((hover_index, _)) = self.hover_state {
                if hover_index == index {
                    self.hover_state = None;
                } else if hover_index > index {
                    // 如果被删除的音符在悬停音符之前，调整索引
                    self.hover_state = Some((hover_index - 1, self.hover_state.unwrap().1));
                }
            }

            // 清除网格缓存以强制重绘
            self.grid_cache.clear();
        }
    }

    /// 删除鼠标位置下的音符（如果存在）
    ///
    /// # Arguments
    /// * `pos` - 鼠标位置
    /// # Returns
    /// 是否删除了音符
    pub fn delete_note_at(&mut self, pos: Point) -> bool {
        if let Some((index, _)) = self.hit_test_note(pos) {
            self.delete_note_by_index(index);
            true
        } else {
            false
        }
    }

    /// 检查音符是否被选中
    pub fn is_note_selected(&self, index: usize) -> bool {
        self.selected_notes.contains(&index)
    }

    /// 获取选中音符的数量
    pub fn selected_notes_count(&self) -> usize {
        self.selected_notes.len()
    }

    /// 清除所有选中
    pub fn clear_selection(&mut self) {
        self.selected_notes.clear();
    }

    /// 删除所有选中的音符
    pub fn delete_selected_notes(&mut self) {
        if self.selected_notes.is_empty() {
            return;
        }

        // 将选中的索引排序，从大到小删除以避免索引变化问题
        let mut indices: Vec<usize> = self.selected_notes.iter().copied().collect();
        indices.sort_by(|a, b| b.cmp(a));

        for index in indices {
            if index < self.notes.len() {
                self.notes.remove(index);
            }
        }

        tracing::debug!("Editor: deleted {} notes", self.selected_notes.len());

        // 更新当前音轨的存储
        if !self.notes.is_empty() {
            self.track_notes
                .insert(self.current_track, self.notes.clone());
        } else {
            self.track_notes.remove(&self.current_track);
        }

        // 清除选中和悬停状态
        self.selected_notes.clear();
        self.hover_state = None;

        // 清除网格缓存以强制重绘
        self.grid_cache.clear();
    }
}
