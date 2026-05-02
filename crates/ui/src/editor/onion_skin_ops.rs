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
        tracing::debug!("Editor: 洋葱皮已启用");
    }

    /// 禁用洋葱皮
    pub fn disable_onion_skin(&mut self) {
        self.onion_skin_config.disable();
        self.grid_cache.clear();
        tracing::debug!("Editor: 洋葱皮已禁用");
    }

    /// 切换洋葱皮开关
    pub fn toggle_onion_skin(&mut self) {
        self.onion_skin_config.toggle();
        self.grid_cache.clear();
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

    /// 设置音轨的洋葱皮颜色
    pub fn set_onion_skin_color(&mut self, track_idx: usize, color: iced_core::Color) {
        self.onion_skin_config.set_track_color(track_idx, color);
        self.grid_cache.clear();
    }

    /// 获取音轨的洋葱皮颜色
    pub fn get_onion_skin_color(&self, track_idx: usize) -> iced_core::Color {
        self.onion_skin_config.get_track_color(track_idx)
    }

    /// 设置洋葱皮透明度
    pub fn set_onion_skin_opacity(&mut self, opacity: f32) {
        self.onion_skin_config.set_opacity(opacity);
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
        self.grid_cache.clear();
    }

    /// 从洋葱皮移除音轨
    pub fn remove_onion_skin_track(&mut self, track_idx: usize) {
        self.onion_skin_config.remove_visible_track(track_idx);
        self.grid_cache.clear();
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

    /// 获取洋葱皮音符实例（用于其他音轨的音符显示）
    /// 音符直接送入 wgpu 渲染管线，GPU compute shader 负责视锥裁剪
    pub fn get_onion_skin_instances(
        &mut self,
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

        // 先将所有音符做成 NoteInstance（GPU shader 负责裁剪）
        // 使用 closure 构建实例列表，同时处理 cache hit/miss
        let make_instances =
            |notes: &im::Vector<Note>, color: iced_core::Color| -> Vec<NoteInstance> {
                let mut instances = Vec::with_capacity(notes.len());
                for note in notes.iter() {
                    instances.push(note.to_instance(color));
                }
                instances
            };

        let color = self.onion_skin_config.get_track_color(track_idx);

        // 先查 track_notes 缓存
        if let Some(cached) = self.track_notes.get(&track_idx) {
            if cached.is_empty() {
                return Vec::new();
            }
            return make_instances(cached, color);
        }

        // 缓存未命中 → 从 document 加载并缓存
        let Some(doc) = self.document.as_ref() else {
            return Vec::new();
        };
        if track_idx as u16 >= doc.track_count() as u16 {
            return Vec::new();
        }
        if doc.track_note_count(track_idx as u16) == 0 {
            return Vec::new();
        }
        let raw = doc.get_track_notes(track_idx as u16);
        if raw.is_empty() {
            return Vec::new();
        }

        let mut notes: im::Vector<Note> = im::Vector::new();
        for (tick, key, length, velocity, channel) in &raw {
            notes.push_back(
                Note::new(*tick, *key as u16, *length)
                    .with_velocity(*velocity)
                    .with_channel(*channel),
            );
        }
        self.track_notes.insert(track_idx, notes.clone());

        make_instances(&notes, color)
    }

    /// 获取所有洋葱皮音符实例（视口范围内）
    ///
    /// 直接从 MidiDocument 查询，利用预排序事件的二分查找，
    /// 零额外内存缓存，零数据拷贝。
    pub fn get_all_onion_skin_instances_in_range(
        &mut self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
    ) -> Vec<NoteInstance> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        let Some(doc) = self.document.as_ref() else {
            return Vec::new();
        };

        const ONION_SKIN_SEARCH_EXTENSION: f32 = 19200.0;
        let search_start = (visible_tick_start - ONION_SKIN_SEARCH_EXTENSION).max(0.0);
        let search_end = visible_tick_end + ONION_SKIN_SEARCH_EXTENSION;

        let track_indices = self.collect_visible_track_indices(track_onion_states);
        let mut all_instances = Vec::new();

        for track_idx in track_indices {
            let color = self.onion_skin_config.get_track_color(track_idx);
            let color_arr = super::note::color_to_array(color);

            // 直接从 document 查询视口范围内的音符
            let raw = doc.get_track_notes_in_range(track_idx as u16, search_start, search_end);
            if raw.is_empty() {
                continue;
            }

            // 转换为 NoteInstance（只保留 key 在可见范围内的）
            for &(tick, key, length, _vel, _ch) in &raw {
                let key_u16 = key as u16;
                if key_u16 >= visible_key_min && key_u16 <= visible_key_max {
                    if tick + length >= visible_tick_start && tick <= visible_tick_end {
                        all_instances.push(NoteInstance::new(tick, key as f32, length, color_arr));
                    }
                }
            }
        }

        all_instances
    }

    /// 从 document 加载音轨音符到 track_notes 缓存
    fn load_track_notes_from_document(&mut self, track_idx: usize) {
        let Some(doc) = self.document.as_ref() else {
            return;
        };
        if track_idx as u16 >= doc.track_count() as u16 {
            return;
        }
        if doc.track_note_count(track_idx as u16) == 0 {
            self.track_notes.insert(track_idx, im::Vector::new());
            return;
        }
        let raw = doc.get_track_notes(track_idx as u16);
        if raw.is_empty() {
            self.track_notes.insert(track_idx, im::Vector::new());
            return;
        }

        let mut notes: im::Vector<Note> = im::Vector::new();
        for (tick, key, length, velocity, channel) in &raw {
            notes.push_back(
                Note::new(*tick, *key as u16, *length)
                    .with_velocity(*velocity)
                    .with_channel(*channel),
            );
        }
        self.track_notes.insert(track_idx, notes);
    }

    /// 获取所有洋葱皮音符实例（所有其他音轨）
    /// 音符全部送入 wgpu 管线，GPU compute shader 负责视锥裁剪
    pub fn get_all_onion_skin_instances(
        &mut self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
    ) -> Vec<NoteInstance> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        let mut track_indices: Vec<usize> = track_onion_states
            .iter()
            .filter(|(_, is_enabled)| **is_enabled)
            .map(|(&idx, _)| idx)
            .filter(|&idx| idx != self.current_track)
            .collect();

        track_indices.sort();

        let mut all_instances = Vec::new();
        for track_idx in track_indices {
            if let Some(&is_enabled) = track_onion_states.get(&track_idx) {
                all_instances.extend(self.get_onion_skin_instances(track_idx, is_enabled));
            }
        }

        all_instances
    }
}
