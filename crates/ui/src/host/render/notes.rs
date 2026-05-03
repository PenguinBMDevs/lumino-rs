use crate::host::Host;
use rayon::prelude::*;

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

        // 获取双缓冲的后缓冲区写入引用
        let instances = unsafe { self.render_cache.note_instances_buffer.write_buffer() };
        instances.clear();

        // 预分配容量
        let onion_skin_count: usize = notes.len() + onion_instances.len() + 1;
        instances.reserve(onion_skin_count);

        let onion_count = onion_instances.len();
        let main_count = notes.len();
        let drawing_count = if matches!(edit_state, crate::editor::EditState::Drawing { .. }) {
            1
        } else {
            0
        };

        // 添加主要音符（全部送入 GPU，由 shader 裁剪）
        const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];
        Self::add_notes_to_instances(instances, notes, DEFAULT_NOTE_COLOR);

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
        self.render_cache.note_instances_version = self.render_cache.note_instances_buffer.swap();
    }

    /// 将音符添加到实例列表
    ///
    /// 优化：
    /// 1. 使用 fold + reduce 模式，避免中间 Vec 分配
    /// 2. 直接使用索引访问，避免创建引用 Vec
    pub(super) fn add_notes_to_instances(
        instances: &mut Vec<lumino_gfx::NoteInstance>,
        notes: &im::Vector<crate::editor::note::Note>,
        color: [f32; 4],
    ) {
        if notes.len() > super::PARALLEL_THRESHOLD {
            // 大数据量使用并行处理 - 使用 fold + reduce 减少内存分配
            let note_count = notes.len();
            let parallel_instances: Vec<lumino_gfx::NoteInstance> = (0..note_count)
                .into_par_iter()
                .fold(
                    || Vec::with_capacity(note_count / rayon::current_num_threads() + 1),
                    |mut local, i| {
                        // SAFETY: i 在 0..note_count 范围内
                        let note = unsafe { notes.get(i).unwrap_unchecked() };
                        local.push(lumino_gfx::NoteInstance::new(
                            note.tick,
                            note.key as f32,
                            note.length,
                            color,
                        ));
                        local
                    },
                )
                .reduce(
                    || Vec::new(),
                    |mut a, b| {
                        a.extend(b);
                        a
                    },
                );
            instances.extend(parallel_instances);
        } else {
            // 小数据量使用串行处理
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
