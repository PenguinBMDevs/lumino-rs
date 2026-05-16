use crate::host::Host;
use lumino_gfx::NoteInstance;

impl Host {
    /// 全量构建所有主音轨 NoteInstance 到缓存（视口无关，跨帧复用）
    ///
    /// 将 im::Vector<Note> 全量转换为 Vec<NoteInstance> 并缓存。
    /// 仅在 note_data_changed 时调用；viewport_changed 时用 filter 代替。
    ///
    /// 返回全量音符数量（用于日志）。
    fn build_all_main_note_instances(&mut self, packed_color: u32) -> usize {
        puffin::profile_scope!("build_all_notes");
        let notes = &self.root.editor.editor_state.data.notes;
        let cache = &mut self.render_ctx.render_cache.cached_all_main_note_instances;
        cache.clear();
        cache.reserve(notes.len());

        if notes.len() > 5000 {
            use rayon::prelude::*;
            let note_refs: Vec<&crate::editor::note::Note> = notes.iter().collect();
            let mut parallel_result: Vec<NoteInstance> = note_refs
                .par_iter()
                .map(|&n| NoteInstance {
                    position: [n.tick, n.key as f32],
                    size_x: n.length,
                    color_packed: packed_color,
                })
                .collect();
            cache.append(&mut parallel_result);
        } else {
            for note in notes.iter() {
                cache.push(NoteInstance {
                    position: [note.tick, note.key as f32],
                    size_x: note.length,
                    color_packed: packed_color,
                });
            }
        }

        notes.len()
    }

    /// 从 cached_all_main_note_instances 过滤出可见范围到 cached_main_note_instances
    ///
    /// 二分查找（tick 单调）+ 并行 filter。约 0.5-2ms / 百万音符。
    /// 返回可见音符数量。
    pub(super) fn filter_visible_from_cache(
        &mut self,
        visible_tick_start: f32,
        visible_tick_end: f32,
    ) -> usize {
        puffin::profile_scope!("filter_visible");
        let all = &self.render_ctx.render_cache.cached_all_main_note_instances;
        if all.is_empty() {
            tracing::warn!(
                "filter_visible_from_cache: cached_all_main_note_instances is empty! \
                 This means build_all was skipped or notes changed without rebuild."
            );
            self.render_ctx
                .render_cache
                .cached_main_note_instances
                .clear();
            return 0;
        }

        // 二分查找可见范围（position[0] = tick，严格单调）
        let range_start = all.partition_point(|n| n.position[0] + n.size_x < visible_tick_start);
        let range_end = all.partition_point(|n| n.position[0] <= visible_tick_end);
        let visible_count = range_end - range_start;

        let cache = &mut self.render_ctx.render_cache.cached_main_note_instances;
        if visible_count > 5000 {
            use rayon::prelude::*;
            let mut parallel_result: Vec<NoteInstance> = all[range_start..range_end]
                .par_iter()
                .filter(|n| n.position[0] + n.size_x >= visible_tick_start)
                .copied()
                .collect();
            cache.clear();
            cache.append(&mut parallel_result);
        } else {
            cache.clear();
            cache.reserve(visible_count);
            for n in all[range_start..range_end].iter() {
                if n.position[0] + n.size_x >= visible_tick_start {
                    cache.push(*n);
                }
            }
        }

        cache.len()
    }

    /// 全量重建所有音符实例（主音轨全量缓存 + 可见过滤 + 洋葱皮 + 绘制中音符）
    ///
    /// note_data_changed 时调用：构建全量缓存（一次 im::Vector 迭代），
    /// 然后过滤可见范围。viewport 变化时不调此函数，改用 filter_visible_from_cache。
    pub(super) fn update_all_note_instances_fast(
        &mut self,
        visible_tick_start: f32,
        visible_tick_end: f32,
    ) {
        puffin::profile_function!();

        let max_key = self
            .root
            .editor
            .editor_state
            .view
            .visible_key_count
            .saturating_sub(1);
        let visible_key_min = 0u16;
        let visible_key_max = max_key;

        // 获取洋葱皮实例（范围内查询）
        let onion_states = self.root.sidebar.get_onion_skin_states();
        let onion_instances = self.root.editor.get_all_onion_skin_instances_in_range(
            &onion_states,
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
        );
        self.render_ctx.render_cache.cached_onion_instances = onion_instances;
        let onion_count = self.render_ctx.render_cache.cached_onion_instances.len();

        // 预计算 packed color
        const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];
        let packed_color = lumino_gfx::pack_color(DEFAULT_NOTE_COLOR);

        // 第一步：全量构建（写入全量、视口无关的数据到 buffer）
        self.build_all_main_note_instances(packed_color);
        let all_count = self
            .render_ctx
            .render_cache
            .cached_all_main_note_instances
            .len();
        let _ = self.filter_visible_from_cache(visible_tick_start, visible_tick_end);

        // 获取编辑器数据引用
        let edit_state = &self.root.editor.editor_state.interaction.edit_state;
        let default_note_length = self.root.editor.editor_state.view.default_note_length;
        let snap_precision = self.root.editor.editor_state.view.snap_precision;

        // 第二步：写入 SwappableBuffer（全量主音轨，视口无关）
        // 渲染线程只在 version 变化时 upload → 滚动时不触发 → 省掉 103ms
        let instance_count = all_count + onion_count + 1;
        let instances = unsafe {
            self.render_ctx
                .render_cache
                .note_instances_buffer
                .write_buffer()
        };
        instances.clear();
        instances.reserve(instance_count);
        instances.extend_from_slice(&self.render_ctx.render_cache.cached_all_main_note_instances);
        instances.extend_from_slice(&self.render_ctx.render_cache.cached_onion_instances);
        Self::add_drawing_note_to_instances(
            instances,
            edit_state,
            default_note_length,
            snap_precision,
        );

        tracing::debug!(
            "update_all_note_instances_fast: total={}, all={}, onion={}",
            instance_count,
            all_count,
            onion_count,
        );

        self.render_ctx.render_cache.note_instances_version =
            self.render_ctx.render_cache.note_instances_buffer.swap();
    }

    /// 仅重建主音轨（全量 + 可见过滤），复用缓存的洋葱皮
    ///
    /// 当音符数据变化但视口未变时调用。
    pub(super) fn rebuild_main_note_instances_only(
        &mut self,
        visible_tick_start: f32,
        visible_tick_end: f32,
    ) {
        puffin::profile_function!();

        let onion_count = self.render_ctx.render_cache.cached_onion_instances.len();

        const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];
        let packed_color = lumino_gfx::pack_color(DEFAULT_NOTE_COLOR);

        // 第一步：全量构建（写入全量、视口无关的数据到 buffer）
        self.build_all_main_note_instances(packed_color);
        let all_count = self
            .render_ctx
            .render_cache
            .cached_all_main_note_instances
            .len();
        let _ = self.filter_visible_from_cache(visible_tick_start, visible_tick_end);

        // 获取编辑器数据引用
        let edit_state = &self.root.editor.editor_state.interaction.edit_state;
        let default_note_length = self.root.editor.editor_state.view.default_note_length;
        let snap_precision = self.root.editor.editor_state.view.snap_precision;

        // 第二步：写入 SwappableBuffer（全量主音轨，视口无关）
        let instance_count = all_count + onion_count + 1;
        let instances = unsafe {
            self.render_ctx
                .render_cache
                .note_instances_buffer
                .write_buffer()
        };
        instances.clear();
        instances.reserve(instance_count);
        instances.extend_from_slice(&self.render_ctx.render_cache.cached_all_main_note_instances);
        instances.extend_from_slice(&self.render_ctx.render_cache.cached_onion_instances);
        Self::add_drawing_note_to_instances(
            instances,
            edit_state,
            default_note_length,
            snap_precision,
        );

        tracing::debug!(
            "rebuild_main_note_instances_only: total={}, all={}, onion={}",
            instance_count,
            all_count,
            onion_count,
        );

        self.render_ctx.render_cache.note_instances_version =
            self.render_ctx.render_cache.note_instances_buffer.swap();
    }

    /// 从缓存快速写入 SwappableBuffer（视口/主音轨不变时使用）
    ///
    /// 接受可选的绘制中音符参数（Copy 类型），避免从 self 借出 edit_state 导致借用冲突。
    pub(super) fn write_cached_instances_to_buffer(
        &mut self,
        drawing_note: Option<(f32, u16, f32)>,
        default_note_length: f32,
        snap_precision: f32,
    ) {
        let main_count = self
            .render_ctx
            .render_cache
            .cached_main_note_instances
            .len();
        let onion_count = self.render_ctx.render_cache.cached_onion_instances.len();
        let instance_count = main_count + onion_count + if drawing_note.is_some() { 1 } else { 0 };

        let instances = unsafe {
            self.render_ctx
                .render_cache
                .note_instances_buffer
                .write_buffer()
        };
        instances.clear();
        instances.reserve(instance_count);
        instances.extend_from_slice(&self.render_ctx.render_cache.cached_main_note_instances);
        instances.extend_from_slice(&self.render_ctx.render_cache.cached_onion_instances);

        // 添加绘制中音符（如果有）
        if let Some((start_tick, key, current_tick)) = drawing_note {
            const DRAWING_NOTE_COLOR_PACKED: u32 = {
                let r = (0.4f32 * 255.0) as u32;
                let g = (0.8f32 * 255.0) as u32;
                let b = (1.0f32 * 255.0) as u32;
                let a = (1.0f32 * 255.0) as u32;
                (r << 24) | (g << 16) | (b << 8) | a
            };

            let (tick, length) = if current_tick > start_tick {
                (start_tick, current_tick - start_tick)
            } else if current_tick < start_tick {
                (current_tick, start_tick - current_tick)
            } else {
                (start_tick, default_note_length)
            };
            let length = length.max(snap_precision);
            instances.push(NoteInstance {
                position: [tick, key as f32],
                size_x: length,
                color_packed: DRAWING_NOTE_COLOR_PACKED,
            });
        }

        self.render_ctx.render_cache.note_instances_version =
            self.render_ctx.render_cache.note_instances_buffer.swap();
    }

    /// 添加正在绘制的音符到实例列表
    pub(super) fn add_drawing_note_to_instances(
        instances: &mut Vec<lumino_gfx::NoteInstance>,
        edit_state: &crate::editor::EditState,
        default_note_length: f32,
        snap_precision: f32,
    ) {
        const DRAWING_NOTE_COLOR_PACKED: u32 = {
            let r = (0.4f32 * 255.0) as u32;
            let g = (0.8f32 * 255.0) as u32;
            let b = (1.0f32 * 255.0) as u32;
            let a = (1.0f32 * 255.0) as u32;
            (r << 24) | (g << 16) | (b << 8) | a
        };

        if let crate::editor::EditState::Drawing {
            start_tick,
            key,
            current_tick,
        } = edit_state
        {
            let (tick, length) = if *current_tick > *start_tick {
                (*start_tick, *current_tick - *start_tick)
            } else if *current_tick < *start_tick {
                (*current_tick, *start_tick - *current_tick)
            } else {
                (*start_tick, default_note_length)
            };
            let length = length.max(snap_precision);

            instances.push(NoteInstance {
                position: [tick, *key as f32],
                size_x: length,
                color_packed: DRAWING_NOTE_COLOR_PACKED,
            });
        }
    }
}
