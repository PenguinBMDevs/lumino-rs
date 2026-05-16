use crate::host::Host;
use lumino_gfx::NoteInstance;

impl Host {
    /// 构建 cached_main_note_instances（音轨主音符 → NoteInstance 双缓冲）
    ///
    /// 返回主音轨音符数量（用于调用方日志）。
    fn build_cached_main_note_instances(&mut self, packed_color: u32) -> usize {
        let notes = &self.root.editor.editor_state.data.notes;
        let cache = &mut self.render_ctx.render_cache.cached_main_note_instances;
        cache.clear();
        cache.reserve(notes.len());

        if notes.len() > 5000 {
            use rayon::prelude::*;
            let note_refs: Vec<&crate::editor::note::Note> = notes.iter().collect();
            let mut parallel_result: Vec<NoteInstance> = note_refs
                .par_iter()
                .map(|&note| NoteInstance {
                    position: [note.tick, note.key as f32],
                    size_x: note.length,
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

    /// 全量重建所有音符实例（主音轨 + 洋葱皮 + 绘制中音符）
    ///
    /// 主音轨音符全量送入 GPU（GPU compute shader 负责裁剪），
    /// 洋葱皮音符只构建视口范围内，使用 Document 范围查询。
    /// 同时更新 cached_onion_instances 供 note_data_only 路径使用。
    pub(super) fn update_all_note_instances_fast(&mut self) {
        puffin::profile_function!();

        // 计算视口 tick/key 范围（用于洋葱皮过滤）
        let editor = &self.root.editor;
        let es = &editor.editor_state;
        let canvas_width = es.canvas.size.x;
        let keyboard_width = es.view.keyboard_width;
        let visible_tick_start = (es.view.scroll_x / es.view.zoom_x).max(0.0);
        let visible_tick_end = ((es.view.scroll_x + canvas_width - keyboard_width)
            / es.view.zoom_x)
            .max(visible_tick_start);
        let max_key = es.view.visible_key_count.saturating_sub(1);
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

        // 缓存洋葱皮实例供 note_data_only 路径复用
        self.render_ctx.render_cache.cached_onion_instances = onion_instances;

        let onion_count = self.render_ctx.render_cache.cached_onion_instances.len();

        // 预计算 packed color
        const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];
        let packed_color = lumino_gfx::pack_color(DEFAULT_NOTE_COLOR);

        // 第一步：构建 cached_main_note_instances（获取主音轨数量）
        // 注：build 需要 &mut self，必须在借用 edit_state 之前调用
        let main_count = self.build_cached_main_note_instances(packed_color);

        // 获取编辑器数据引用（build 之后，避免借用冲突）
        let edit_state = &self.root.editor.editor_state.interaction.edit_state;
        let default_note_length = self.root.editor.editor_state.view.default_note_length;
        let snap_precision = self.root.editor.editor_state.view.snap_precision;

        // 第二步：写入 SwappableBuffer
        let instance_count = main_count + onion_count + 1;
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
        Self::add_drawing_note_to_instances(
            instances,
            edit_state,
            default_note_length,
            snap_precision,
        );

        tracing::debug!(
            "update_all_note_instances_fast: total={}, main={}, onion={}",
            instance_count,
            main_count,
            onion_count,
        );

        // 交换双缓冲区
        self.render_ctx.render_cache.note_instances_version =
            self.render_ctx.render_cache.note_instances_buffer.swap();
    }

    /// 仅重建主音轨音符实例（复用缓存的洋葱皮）
    ///
    /// 当音符数据变化但视口未变时调用。视口未变→洋葱皮范围相同→
    /// 直接用 cached_onion_instances，避免不必要的 Document 范围查询。
    pub(super) fn rebuild_main_note_instances_only(&mut self) {
        puffin::profile_function!();

        let onion_count = self.render_ctx.render_cache.cached_onion_instances.len();

        // 预计算 packed color
        const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];
        let packed_color = lumino_gfx::pack_color(DEFAULT_NOTE_COLOR);

        // 第一步：构建 cached_main_note_instances（获取主音轨数量）
        // 注：build 需要 &mut self，必须在借用 edit_state 之前调用
        let main_count = self.build_cached_main_note_instances(packed_color);

        // 获取编辑器数据引用（build 之后，避免借用冲突）
        let edit_state = &self.root.editor.editor_state.interaction.edit_state;
        let default_note_length = self.root.editor.editor_state.view.default_note_length;
        let snap_precision = self.root.editor.editor_state.view.snap_precision;

        // 第二步：写入 SwappableBuffer（复用 cached_onion_instances）
        let instance_count = main_count + onion_count + 1;
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
        Self::add_drawing_note_to_instances(
            instances,
            edit_state,
            default_note_length,
            snap_precision,
        );

        tracing::debug!(
            "rebuild_main_note_instances_only: total={}, main={}, onion={}",
            instance_count,
            main_count,
            onion_count,
        );

        // 交换双缓冲区
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
