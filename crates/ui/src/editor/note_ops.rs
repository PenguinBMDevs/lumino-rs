use iced_core::Point;

use crate::constants::editor::NOTE_EDGE_THRESHOLD_PX;

use super::{Editor, HitType};

impl Editor {
    /// 碰撞检测：测试点击位置是否命中音符
    pub fn hit_test_note(&self, pos: Point) -> Option<(usize, HitType)> {
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);

        for (i, note) in self.editor_state.data.notes.iter().enumerate().rev() {
            if note.key == key && tick >= note.tick && tick <= note.tick + note.length {
                let start_dist = (tick - note.tick).abs();
                let end_dist = (tick - (note.tick + note.length)).abs();
                let edge_threshold = NOTE_EDGE_THRESHOLD_PX / self.editor_state.view.zoom_x;

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
        if index < self.editor_state.data.notes.len() {
            // Push to history before modifying
            self.push_history();

            let note = self.editor_state.data.notes.remove(index);
            tracing::debug!(
                "Editor: deleted note at index {} (tick={}, key={})",
                index,
                note.tick,
                note.key
            );

            // 更新当前音轨的存储
            if !self.editor_state.data.notes.is_empty() {
                self.editor_state.data.track_notes.insert(
                    self.editor_state.data.current_track,
                    self.editor_state.data.notes.clone(),
                );
            } else {
                // 如果音符列表为空，从 track_notes 中移除该音轨
                self.editor_state
                    .data
                    .track_notes
                    .remove(&self.editor_state.data.current_track);
            }

            // 清除悬停状态（如果被删除的音符正好是悬停的）
            let interaction = &mut self.editor_state.interaction;
            if let Some((hover_index, _)) = interaction.hover_state {
                if hover_index == index {
                    interaction.hover_state = None;
                } else if hover_index > index {
                    // 如果被删除的音符在悬停音符之前，调整索引
                    if let Some((_, second)) = interaction.hover_state {
                        interaction.hover_state = Some((hover_index - 1, second));
                    }
                }
            }

            // 标记音符数据已变化（音符由 wgpu 渲染，不需要清 grid cache）
            self.mark_notes_changed();
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
        self.editor_state
            .interaction
            .selected_notes
            .contains(&index)
    }

    /// 获取选中音符的数量
    pub fn selected_notes_count(&self) -> usize {
        self.editor_state.interaction.selected_notes.len()
    }

    /// 清除所有选中
    pub fn clear_selection(&mut self) {
        self.editor_state.interaction.selected_notes.clear();
    }

    /// 删除所有选中的音符
    pub fn delete_selected_notes(&mut self) {
        // 先复制选中的索引，避免之后修改 self 时的借用冲突
        let mut indices: Vec<usize> = self
            .editor_state
            .interaction
            .selected_notes
            .iter()
            .copied()
            .collect();
        if indices.is_empty() {
            return;
        }

        // Push to history before modifying
        self.push_history();

        // 从大到小排序，避免索引变化问题
        indices.sort_by(|a, b| b.cmp(a));

        for &index in &indices {
            if index < self.editor_state.data.notes.len() {
                self.editor_state.data.notes.remove(index);
            }
        }

        tracing::debug!("Editor: 删除了 {} 个音符", indices.len());

        // 更新当前音轨的存储
        if !self.editor_state.data.notes.is_empty() {
            self.editor_state.data.track_notes.insert(
                self.editor_state.data.current_track,
                self.editor_state.data.notes.clone(),
            );
        } else {
            self.editor_state
                .data
                .track_notes
                .remove(&self.editor_state.data.current_track);
        }

        // 清除选中和悬停状态
        self.editor_state.interaction.selected_notes.clear();
        self.editor_state.interaction.hover_state = None;

        // 标记音符数据已变化（音符由 wgpu 渲染，不需要清 grid cache）
        self.mark_notes_changed();
    }

    /// 获取框选范围内的所有音符索引
    ///
    /// 将屏幕坐标的选择框转换为 tick/key 范围，然后收集范围内的音符
    pub(super) fn get_notes_in_selection_box(
        &self,
        start_pos: Point,
        current_pos: Point,
    ) -> Vec<usize> {
        let min_x = start_pos.x.min(current_pos.x);
        let max_x = start_pos.x.max(current_pos.x);
        let min_y = start_pos.y.min(current_pos.y);
        let max_y = start_pos.y.max(current_pos.y);

        let tick_start = self.x_to_tick(min_x);
        let tick_end = self.x_to_tick(max_x);
        let key_min = self.y_to_key(max_y);
        let key_max = self.y_to_key(min_y);

        let mut indices = Vec::new();
        for (i, note) in self.editor_state.data.notes.iter().enumerate() {
            let note_end = note.tick + note.length;
            if note.key >= key_min
                && note.key <= key_max
                && note.tick < tick_end
                && note_end > tick_start
            {
                indices.push(i);
            }
        }
        indices
    }

    /// 删除框选范围内的所有音符（橡皮工具用）
    ///
    /// 不依赖 selected_notes，直接从 EditorState 的音符数据中删除。
    pub(super) fn delete_notes_in_selection_box(&mut self, start_pos: Point, current_pos: Point) {
        let indices = self.get_notes_in_selection_box(start_pos, current_pos);
        if indices.is_empty() {
            return;
        }

        self.push_history();

        // 从大到小排序，避免索引变化问题
        let mut sorted = indices;
        sorted.sort_by(|a, b| b.cmp(a));
        sorted.dedup();

        for &index in &sorted {
            self.editor_state.data.notes.remove(index);
        }

        tracing::debug!("Editor: 框选删除了 {} 个音符", sorted.len());

        // 更新 track_notes 缓存
        if !self.editor_state.data.notes.is_empty() {
            self.editor_state.data.track_notes.insert(
                self.editor_state.data.current_track,
                self.editor_state.data.notes.clone(),
            );
        } else {
            self.editor_state
                .data
                .track_notes
                .remove(&self.editor_state.data.current_track);
        }

        self.editor_state.interaction.selected_notes.clear();
        self.editor_state.interaction.hover_state = None;
        self.mark_notes_changed();
    }

    /// 选择全部音符
    pub fn select_all_notes(&mut self) {
        self.editor_state.interaction.selected_notes.clear();
        self.editor_state
            .interaction
            .selected_notes
            .extend(0..self.editor_state.data.notes.len());
        // 选择框是 Canvas 上实时渲染的叠加层，不需要清 grid cache
    }
}
