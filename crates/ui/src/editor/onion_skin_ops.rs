use crate::editor::Editor;
use crate::editor::note::Note;
use lumino_gfx::NoteInstance;

impl Editor {
    /// 获取洋葱皮配置的可变引用
    pub fn onion_skin_config_mut(&mut self) -> &mut super::OnionSkinConfig {
        &mut self.onion_skin_config
    }

    /// 获取洋葱皮配置的引用
    pub fn onion_skin_config(&self) -> &super::OnionSkinConfig {
        &self.onion_skin_config
    }

    /// 启用洋葱皮
    pub fn enable_onion_skin(&mut self) {
        self.onion_skin_config.enable();
        self.grid_cache.clear();
        // 标记所有已加载的音轨需重新生成缓存
        let tracks: Vec<usize> = self.track_notes.keys().copied().collect();
        self.onion_skin_dirty.borrow_mut().extend(tracks);
        tracing::debug!("Editor: 洋葱皮已启用");
    }

    /// 禁用洋葱皮
    pub fn disable_onion_skin(&mut self) {
        self.onion_skin_config.disable();
        self.grid_cache.clear();
        // 清空缓存和脏标记
        self.onion_skin_cache.borrow_mut().clear();
        self.onion_skin_dirty.borrow_mut().clear();
        tracing::debug!("Editor: 洋葱皮已禁用");
    }

    /// 切换洋葱皮开关
    pub fn toggle_onion_skin(&mut self) {
        self.onion_skin_config.toggle();
        self.grid_cache.clear();
        if self.onion_skin_config.is_enabled() {
            let tracks: Vec<usize> = self.track_notes.keys().copied().collect();
            self.onion_skin_dirty.borrow_mut().extend(tracks);
        } else {
            self.onion_skin_cache.borrow_mut().clear();
            self.onion_skin_dirty.borrow_mut().clear();
        }
        tracing::info!(
            "Editor: saved {} notes to track {}",
            self.notes.len(),
            self.current_track
        );
    }

    /// 检查洋葱皮是否启用
    pub fn is_onion_skin_enabled(&self) -> bool {
        self.onion_skin_config.is_enabled()
    }

    /// 设置音轨的洋葱皮颜色，同时标记该音轨的缓存为脏
    pub fn set_onion_skin_color(&mut self, track_idx: usize, color: iced_core::Color) {
        self.onion_skin_config.set_track_color(track_idx, color);
        self.onion_skin_dirty.borrow_mut().insert(track_idx);
        self.grid_cache.clear();
    }

    /// 获取音轨的洋葱皮颜色
    pub fn get_onion_skin_color(&self, track_idx: usize) -> iced_core::Color {
        self.onion_skin_config.get_track_color(track_idx)
    }

    /// 设置洋葱皮透明度
    pub fn set_onion_skin_opacity(&mut self, opacity: f32) {
        self.onion_skin_config.set_opacity(opacity);
        // 透明度变化需要重新生成所有缓存的实例（颜色中的 alpha 已烘焙）
        let active: Vec<usize> = self.track_notes.keys().copied().collect();
        self.onion_skin_dirty.borrow_mut().extend(active);
        self.grid_cache.clear();
    }

    /// 获取洋葱皮透明度
    pub fn onion_skin_opacity(&self) -> f32 {
        self.onion_skin_config.opacity()
    }

    /// 设置是否显示所有音轨的洋葱皮
    pub fn set_onion_skin_show_all(&mut self, show_all: bool) {
        self.onion_skin_config.set_show_all_tracks(show_all);
        self.grid_cache.clear();
    }

    /// 添加可见音轨到洋葱皮
    pub fn add_onion_skin_track(&mut self, track_idx: usize) {
        self.onion_skin_config.add_visible_track(track_idx);
        self.onion_skin_dirty.borrow_mut().insert(track_idx);
        self.grid_cache.clear();
    }

    /// 从洋葱皮移除音轨
    pub fn remove_onion_skin_track(&mut self, track_idx: usize) {
        self.onion_skin_config.remove_visible_track(track_idx);
        self.onion_skin_cache.borrow_mut().remove(&track_idx);
        self.onion_skin_dirty.borrow_mut().remove(&track_idx);
        self.grid_cache.clear();
    }

    // ── 洋葱皮实例缓存管理 ──

    /// 标记指定音轨的洋葱皮实例缓存为脏（需要重新生成）
    ///
    /// 在音符数据或颜色发生变化时调用，确保渲染使用最新数据。
    pub fn mark_onion_skin_dirty(&self, track_idx: usize) {
        self.onion_skin_dirty.borrow_mut().insert(track_idx);
    }

    /// 为指定音轨重新生成洋葱皮实例缓存
    ///
    /// 从 `track_notes` 中读取音符数据，使用当前配置的颜色生成 `NoteInstance` 列表。
    /// 生成后自动清除该音轨的脏标记。
    pub fn regenerate_onion_skin_cache(&self, track_idx: usize) {
        let color = self.onion_skin_config.get_track_color(track_idx);
        if let Some(notes) = self.track_notes.get(&track_idx) {
            if notes.is_empty() {
                self.onion_skin_cache.borrow_mut().remove(&track_idx);
            } else {
                let instances: Vec<NoteInstance> =
                    notes.iter().map(|n| n.to_instance(color)).collect();
                self.onion_skin_cache
                    .borrow_mut()
                    .insert(track_idx, instances);
            }
        } else {
            // 音轨没有加载音符数据，清除缓存
            self.onion_skin_cache.borrow_mut().remove(&track_idx);
        }
        self.onion_skin_dirty.borrow_mut().remove(&track_idx);
    }

    /// 重新生成所有脏音轨的洋葱皮缓存
    ///
    /// 在渲染前调用，确保所有音轨的缓存都处于最新状态。
    pub fn regenerate_all_dirty_onion_skin_caches(&self) {
        let dirty: Vec<usize> = self.onion_skin_dirty.borrow().iter().copied().collect();
        for &track_idx in &dirty {
            self.regenerate_onion_skin_cache(track_idx);
        }
    }

    /// 为所有已加载音轨预生成洋葱皮缓存
    ///
    /// 在 MIDI 加载完成后调用，在后台预生成所有音轨的实例缓存。
    pub fn pregenerate_all_onion_skin_caches(&self) {
        let tracks: Vec<usize> = self.track_notes.keys().copied().collect();
        for &track_idx in &tracks {
            if track_idx != self.current_track {
                self.regenerate_onion_skin_cache(track_idx);
            }
        }
        // 标记当前音轨也需生成（切换后自然会生成）
        self.onion_skin_dirty
            .borrow_mut()
            .insert(self.current_track);
    }

    /// 获取所有洋葱皮音符原始数据（用于缓存）
    /// 返回 (tick, key, length, color) 元组，不含屏幕坐标
    pub fn get_onion_skin_notes(
        &self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
    ) -> Vec<(f32, u16, f32, iced_core::Color)> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        let track_indices = self.collect_visible_track_indices(track_onion_states);
        let mut all_notes = Vec::new();

        for track_idx in track_indices {
            let Some(track_notes) = self.collect_track_notes(
                track_idx,
                track_onion_states,
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
            ) else {
                continue;
            };
            all_notes.extend(track_notes);
        }

        all_notes
    }

    /// 收集可见音轨索引
    fn collect_visible_track_indices(
        &self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
    ) -> Vec<usize> {
        let mut indices: Vec<usize> = track_onion_states
            .iter()
            .filter(|(_, is_enabled)| **is_enabled)
            .map(|(&idx, _)| idx)
            .filter(|&idx| idx != self.current_track)
            .collect();
        indices.sort();
        indices
    }

    /// 收集单个音轨的音符
    fn collect_track_notes(
        &self,
        track_idx: usize,
        track_onion_states: &std::collections::HashMap<usize, bool>,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
    ) -> Option<Vec<(f32, u16, f32, iced_core::Color)>> {
        let is_enabled = *track_onion_states.get(&track_idx)?;

        if !self
            .onion_skin_config
            .should_show_track(track_idx, is_enabled)
        {
            return None;
        }

        let notes = self.track_notes.get(&track_idx)?;
        let color = self.onion_skin_config.get_track_color(track_idx);
        const ONION_SKIN_SEARCH_EXTENSION: f32 = 19200.0;
        let search_start = (visible_tick_start - ONION_SKIN_SEARCH_EXTENSION).max(0.0);

        // 确保空间索引存在
        self.ensure_track_spatial_index(track_idx, notes);

        // 查询候选音符
        let indices_map = self.track_note_indices.borrow();
        let index = indices_map.get(&track_idx)?;

        let mut candidates = self.query_cache.borrow_mut();
        index.update_query(
            search_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
            &mut candidates,
        );

        // 过滤并收集音符
        let track_notes: Vec<_> = candidates
            .iter()
            .filter_map(|&i| notes.get(i))
            .filter(|note| {
                Self::note_in_visible_range(
                    note,
                    visible_tick_start,
                    visible_tick_end,
                    visible_key_min,
                    visible_key_max,
                )
            })
            .map(|note| (note.tick, note.key, note.length, color))
            .collect();

        Some(track_notes)
    }

    /// 确保音轨的空间索引存在
    fn ensure_track_spatial_index(&self, track_idx: usize, notes: &im::Vector<Note>) {
        let mut indices_map = self.track_note_indices.borrow_mut();

        if indices_map.contains_key(&track_idx) {
            return;
        }

        let notes_vec: Vec<_> = notes.iter().cloned().collect();
        indices_map.insert(
            track_idx,
            super::spatial_index::NoteSpatialIndex::from_notes(&notes_vec),
        );
    }

    /// 检查音符是否在可见范围内
    fn note_in_visible_range(
        note: &Note,
        tick_start: f32,
        tick_end: f32,
        key_min: u16,
        key_max: u16,
    ) -> bool {
        let in_tick_range = note.tick + note.length >= tick_start && note.tick <= tick_end;
        let in_key_range = note.key >= key_min && note.key <= key_max;
        in_tick_range && in_key_range
    }

    /// 获取洋葱皮音符实例（从缓存获取，若脏则先重新生成）
    pub fn get_onion_skin_instances(
        &self,
        track_idx: usize,
        track_onion_enabled: bool,
    ) -> Vec<NoteInstance> {
        if !self
            .onion_skin_config
            .should_show_track(track_idx, track_onion_enabled)
        {
            return Vec::new();
        }

        if track_idx == self.current_track {
            return Vec::new();
        }

        // 如果该音轨缓存为脏，先重新生成
        if self.onion_skin_dirty.borrow().contains(&track_idx) {
            self.regenerate_onion_skin_cache(track_idx);
        }

        // 从缓存中获取
        self.onion_skin_cache
            .borrow()
            .get(&track_idx)
            .cloned()
            .unwrap_or_default()
    }

    /// 确保指定音轨的音符已加载到 track_notes 缓存中
    ///
    /// 如果该音轨尚未加载，尝试从 MidiDocument 中懒加载。
    /// 由 `update_all_note_instances_fast` 每帧调用，每帧最多加载 2 个新音轨。
    /// 加载后自动标记该音轨的洋葱皮缓存为脏，以便下次渲染时生成。
    pub fn ensure_track_notes_loaded(&mut self, track_idx: usize) {
        if self.track_notes.contains_key(&track_idx) {
            return;
        }
        let Some(ref doc) = self.document else {
            tracing::warn!(
                "ensure_track_notes_loaded: document is None, track={}",
                track_idx
            );
            return;
        };
        if track_idx >= doc.track_count() {
            tracing::warn!(
                "ensure_track_notes_loaded: track_idx {} >= track_count {}",
                track_idx,
                doc.track_count()
            );
            return;
        }

        tracing::info!(
            "ensure_track_notes_loaded: loading track {} (events={})",
            track_idx,
            doc.track_note_count(track_idx as u16)
        );
        let notes = doc.get_track_notes(track_idx as u16);
        if notes.is_empty() {
            self.track_notes.insert(track_idx, im::Vector::new());
            return;
        }

        let mut track_notes: im::Vector<Note> = im::Vector::new();
        for (tick, key, length, velocity, channel) in &notes {
            track_notes.push_back(
                Note::new(*tick, *key as u16, *length)
                    .with_velocity(*velocity)
                    .with_channel(*channel),
            );
        }
        self.track_notes.insert(track_idx, track_notes);

        // 标记该音轨的洋葱皮缓存需要重新生成
        self.onion_skin_dirty.borrow_mut().insert(track_idx);
    }

    /// 获取所有洋葱皮音符实例（从缓存获取，若脏则先重新生成）
    pub fn get_all_onion_skin_instances(
        &self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
    ) -> Vec<NoteInstance> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        // 先重新生成所有脏音轨的缓存
        self.regenerate_all_dirty_onion_skin_caches();

        let cache = self.onion_skin_cache.borrow();

        // 收集需要显示的音轨索引（与原始逻辑相同）
        let mut track_indices: Vec<usize> = track_onion_states
            .iter()
            .filter(|(_, is_enabled)| **is_enabled)
            .map(|(&idx, _)| idx)
            .filter(|&idx| idx != self.current_track)
            .collect();

        track_indices.sort();

        let mut all_instances = Vec::new();
        for track_idx in track_indices {
            let Some(&is_enabled) = track_onion_states.get(&track_idx) else {
                continue;
            };
            if !self
                .onion_skin_config
                .should_show_track(track_idx, is_enabled)
            {
                continue;
            }
            if let Some(instances) = cache.get(&track_idx) {
                all_instances.extend(instances.iter().copied());
            }
        }

        all_instances
    }
}
