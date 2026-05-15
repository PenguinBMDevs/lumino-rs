use rayon::prelude::*;

use crate::host::Host;

impl Host {
    /// 快速更新所有音符实例（双缓冲模式）
    ///
    /// 主音轨音符全量送入 GPU（GPU compute shader 负责裁剪），
    /// 洋葱皮音符只构建视口 ±2 屏范围内，使用空间索引快速过滤。
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
        // key 范围用最大值，空间索引的主过滤靠 tick 范围
        let visible_key_min = 0u16;
        let visible_key_max = max_key;
        // 获取洋葱皮实例（仅在视口范围内，使用空间索引）
        let onion_states = self.root.sidebar.get_onion_skin_states();
        let onion_instances = self.root.editor.get_all_onion_skin_instances_in_range(
            &onion_states,
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
        );

        // 再获取编辑器数据引用（不可变借用）
        let notes = &self.root.editor.editor_state.data.notes;
        let edit_state = &self.root.editor.editor_state.interaction.edit_state;
        let default_note_length = self.root.editor.editor_state.view.default_note_length;
        let snap_precision = self.root.editor.editor_state.view.snap_precision;

        let onion_count = onion_instances.len();
        let main_count = notes.len();
        let drawing_count = if matches!(edit_state, crate::editor::EditState::Drawing { .. }) {
            1
        } else {
            0
        };

        // 添加主要音符（全部送入 GPU，由 shader 裁剪）
        const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];

        // 第一步：写入 cached_main_note_instances（视口变化时免重复迭代 im::Vector）
        // 先释放 cache 的 mutable borrow，再获取 buffer 的 mutable borrow
        let main_instances_clone;
        {
            let cache = &mut self.render_ctx.render_cache.cached_main_note_instances;
            cache.clear();
            cache.reserve(notes.len());
            Self::add_notes_to_instances(cache, notes, DEFAULT_NOTE_COLOR);
            main_instances_clone = cache.clone();
        }

        // 第二步：写入 SwappableBuffer
        let instance_count = notes.len() + onion_instances.len() + 1;
        let instances = unsafe {
            self.render_ctx
                .render_cache
                .note_instances_buffer
                .write_buffer()
        };
        instances.clear();
        instances.reserve(instance_count);
        instances.extend(main_instances_clone);

        // 添加洋葱皮音符（全部送入 GPU，由 shader 裁剪）
        instances.extend(onion_instances);

        // 添加正在绘制的音符
        Self::add_drawing_note_to_instances(
            instances,
            edit_state,
            default_note_length,
            snap_precision,
        );

        tracing::debug!(
            "update_all_note_instances_fast: total={}, main={}, onion={}, drawing={}",
            instances.len(),
            main_count,
            onion_count,
            drawing_count
        );

        // 交换双缓冲区，使新数据对渲染线程可见
        self.render_ctx.render_cache.note_instances_version =
            self.render_ctx.render_cache.note_instances_buffer.swap();
    }

    /// 将音符添加到实例列表
    ///
    /// 优化说明（基于火焰图 96-360ms 瓶颈）：
    /// 1. im::Vector 使用 RRB 树结构，.iter() 顺序遍历均摊 O(1) 每元素，
    ///    远快于随机 get(i) 的 O(log n) 方案。
    /// 2. 大数据量（≥2000 音符）使用 rayon 并行转换：
    ///    - 阶段一：顺序迭代 im::Vector 收集原始 (tick, key, length) 元组（纯拷贝，无分配）
    ///    - 阶段二：par_chunks 并行分块转换为 NoteInstance（CPU 密集）
    pub(super) fn add_notes_to_instances(
        instances: &mut Vec<lumino_gfx::NoteInstance>,
        notes: &im::Vector<crate::editor::note::Note>,
        color: [f32; 4],
    ) {
        const PARALLEL_THRESHOLD: usize = 2000;

        let count = notes.len();
        if count >= PARALLEL_THRESHOLD {
            // 阶段一：顺序迭代 im::Vector，收集原始元组（快路径：3 字段拷贝）
            let mut raw: Vec<(f32, u16, f32)> = Vec::with_capacity(count);
            for note in notes.iter() {
                raw.push((note.tick, note.key, note.length));
            }

            // 阶段二：par_chunks 并行分块转换为 NoteInstance
            let num_threads = rayon::current_num_threads();
            let chunk_size = (count / num_threads).max(1);
            let new_instances: Vec<lumino_gfx::NoteInstance> = raw
                .par_chunks(chunk_size)
                .flat_map(|chunk| {
                    chunk
                        .iter()
                        .map(|&(tick, key, length)| {
                            lumino_gfx::NoteInstance::new(tick, key as f32, length, color)
                        })
                        .collect::<Vec<_>>()
                })
                .collect();

            instances.extend(new_instances);
        } else {
            // 小数据量：顺序处理，避免并行开销
            for note in notes.iter() {
                instances.push(lumino_gfx::NoteInstance::new(
                    note.tick,
                    note.key as f32,
                    note.length,
                    color,
                ));
            }
        }
    }

    /// 添加正在绘制的音符到实例列表
    pub(super) fn add_drawing_note_to_instances(
        instances: &mut Vec<lumino_gfx::NoteInstance>,
        edit_state: &crate::editor::EditState,
        default_note_length: f32,
        snap_precision: f32,
    ) {
        const DRAWING_NOTE_COLOR: [f32; 4] = [0.4, 0.8, 1.0, 1.0];

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

            instances.push(lumino_gfx::NoteInstance::new(
                tick,
                *key as f32,
                length,
                DRAWING_NOTE_COLOR,
            ));
        }
    }
}
