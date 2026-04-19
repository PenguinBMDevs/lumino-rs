use crate::host::Host;
use rayon::prelude::*;

impl Host {
    /// 快速更新所有音符实例（双缓冲模式）
    ///
    /// 使用双缓冲机制实现零拷贝数据传递：
    /// 1. UI 线程写入 Back Buffer
    /// 2. 交换前后缓冲区（原子指针交换，无数据拷贝）
    /// 3. 渲染线程读取 Front Buffer 并上传到 GPU
    ///
    /// 这个模式避免了 CPU 端的视锥裁剪，直接上传所有音符到 GPU
    /// 让 GPU 的 compute shader 处理裁剪，适合超密集音符场景
    pub(super) fn update_all_note_instances_fast(&mut self) {
        puffin::profile_function!();

        // 获取双缓冲的后缓冲区写入引用
        let instances = unsafe { self.render_cache.note_instances_buffer.write_buffer() };
        instances.clear();

        // 获取编辑器数据引用（避免后续借用冲突）
        let notes = &self.root.editor.notes;
        let track_notes = &self.root.editor.track_notes;
        let edit_state = &self.root.editor.edit_state;
        let default_note_length = self.root.editor.state.default_note_length;
        let snap_precision = self.root.editor.state.snap_precision;

        // 预分配容量
        let onion_skin_count: usize = track_notes.values().map(|n| n.len()).sum();
        let total_capacity = notes.len() + onion_skin_count + 1; // +1 for drawing note
        instances.reserve(total_capacity);

        // 添加主要音符
        const DEFAULT_NOTE_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.9];
        Self::add_notes_to_instances(instances, notes, DEFAULT_NOTE_COLOR);

        // 添加洋葱皮音符
        let onion_states = self.root.sidebar.get_onion_skin_states();
        let onion_instances = self.root.editor.get_all_onion_skin_instances(&onion_states);
        instances.extend(onion_instances);

        // 添加正在绘制的音符
        Self::add_drawing_note_to_instances(
            instances,
            edit_state,
            default_note_length,
            snap_precision,
        );

        // 交换双缓冲区，使新数据对渲染线程可见
        self.render_cache.note_instances_version =
            self.render_cache.note_instances_buffer.swap();
    }

    /// 将音符添加到实例列表
    pub(super) fn add_notes_to_instances(
        instances: &mut Vec<lumino_gfx::NoteInstance>,
        notes: &im::Vector<crate::editor::note::Note>,
        color: [f32; 4],
    ) {
        if notes.len() > super::PARALLEL_THRESHOLD {
            // 大数据量使用并行处理
            let notes_vec: Vec<_> = notes.iter().collect();
            let parallel_instances: Vec<lumino_gfx::NoteInstance> = notes_vec
                .par_iter()
                .map(|note| {
                    lumino_gfx::NoteInstance::new(note.tick, note.key as f32, note.length, color)
                })
                .collect();
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
