//! 音符实例收集与渲染
//!
//! 负责将编辑器中的音符数据转换为 `lumino_gfx::NoteInstance`，
//! 供 wgpu 渲染器使用。包含并行/顺序收集路径、ghost 偏移应用、
//! 以及正在绘制/预览音符的实例生成。

use iced_core::Point;
use lumino_gfx::NoteInstance;
use rayon::prelude::*;

use crate::note::color_to_array;
use crate::{EditState, Editor};
use lumino_message::Tool;
use lumino_ui_constants::editor::PREVIEW_NOTE_OPACITY;

impl Editor {
    /// 获取当前需要绘制的音符实例（用于 wgpu 渲染）
    ///
    /// 优化：
    /// 1. 使用 rayon 进行并行处理，将 NoteInstance 转换并行化
    /// 2. 预分配足够的内存，减少重新分配
    /// 3. 利用空间索引的查询缓存，避免重复查询
    /// 4. 使用线程本地存储的中间缓冲区，减少内存分配
    pub fn update_note_instances(
        &self,
        theme: &lumino_ui_core::Theme,
        _sidebar_width: f32,
        instances: &mut Vec<NoteInstance>,
    ) {
        instances.clear();
        let palette = theme.extended_palette();

        // 默认音符颜色（主色弱化）
        let default_color = color_to_array(palette.primary.weak.color);
        // 悬停音符颜色
        let hover_color = color_to_array(palette.primary.base.color);
        // 正在编辑/选中的音符颜色（主色强化）
        let active_color = color_to_array(palette.primary.strong.color);
        // 选中音符的颜色（辅色）
        let selected_color = color_to_array(palette.secondary.strong.color);

        // 使用 editor_state 获取视图/交互/画布状态
        let es = &self.editor_state;
        let view = &es.view;
        let canvas = &es.canvas;

        let viewport_width = canvas.size_x - view.keyboard_width;
        let viewport_height = canvas.size_y - view.ruler_height;

        // 计算可见 tick 范围（使用 scroll_x，不含 canvas_offset）
        let visible_tick_start = (view.scroll_x / view.zoom_x).max(0.0);
        let visible_tick_end =
            ((view.scroll_x + viewport_width) / view.zoom_x).max(visible_tick_start);

        // 计算可见 key 范围（使用 scroll_y，含 max_key_index）
        let max_key_index = (view.visible_key_count - 1) as f32;

        // key_top 对应屏幕上方 (Y = ruler_height)，值最大（高音）
        let key_top_f32 = max_key_index - (view.scroll_y / view.zoom_y);
        // key_bottom 对应屏幕下方 (Y = canvas_size.y)，值最小（低音）
        let key_bottom_f32 = max_key_index - ((view.scroll_y + viewport_height) / view.zoom_y);

        let visible_key_max = key_top_f32.ceil() as u16 + 1; // 多取 1 个作为缓冲
        let visible_key_min = (key_bottom_f32.floor().max(0.0) as u16).saturating_sub(1); // 多取 1 个作为缓冲

        // 重建空间索引（仅当数据变化时）
        self.rebuild_spatial_index_if_dirty();

        // 查询可见范围内的音符（每帧渲染时执行，确保视图变化时刷新）
        let candidate_count = self.query_visible_candidate_count(
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
        );

        // 预分配内存
        instances.reserve(candidate_count);

        // 直接顺序处理（小数据量）：顺序访问性能更好，且避免内存分配
        // 只有当候选音符数量超过阈值时，才使用并行处理
        const PARALLEL_THRESHOLD: usize = 500;

        let note_instances = if candidate_count >= PARALLEL_THRESHOLD {
            // 大数据量：使用并行处理
            self.collect_parallel_instances(
                visible_tick_end,
                candidate_count,
                default_color,
                hover_color,
                active_color,
                selected_color,
            )
        } else {
            // 小数据量：顺序处理，避免并行开销
            self.collect_sequential_instances(
                visible_tick_end,
                candidate_count,
                default_color,
                hover_color,
                active_color,
                selected_color,
            )
        };

        instances.extend(note_instances);

        // 渲染正在绘制的音符与预览音符
        self.push_drawing_and_preview_instances(instances, default_color, active_color);
    }

    /// 仅当空间索引脏时，从 `im::Vector` 直接构建音符空间索引。
    fn rebuild_spatial_index_if_dirty(&self) {
        // 优化：使用 from_note_refs 直接从 im::Vector 构建，避免克隆 Note 到 Vec<Note>
        if self.spatial.note_index_dirty.get() {
            let notes = &self.editor_state.data.notes;
            let note_refs: Vec<lumino_core::NoteRef> = notes
                .iter()
                .enumerate()
                .map(|(i, n)| lumino_core::NoteRef {
                    tick: n.tick,
                    key: n.key,
                    length: n.length,
                    index: i,
                })
                .collect();
            *self.spatial.note_index.borrow_mut() = Some(
                crate::spatial_index::NoteSpatialIndex::from_note_refs(&note_refs),
            );
            self.spatial.note_index_dirty.set(false);
            tracing::debug!(
                "Editor: rebuild spatial index for {} notes",
                self.editor_state.data.notes.len()
            );
        }
    }

    /// 用当前视口范围刷新空间索引查询缓存，并返回候选音符数量。
    fn query_visible_candidate_count(
        &self,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
    ) -> usize {
        let mut cache = self.spatial.query_cache.borrow_mut();
        if let Some(index) = &*self.spatial.note_index.borrow() {
            index.update_query(
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                &mut cache,
            );
        } else {
            cache.clear();
        }
        cache.len()
    }

    /// 大数据量路径：并行收集可见音符实例（fold + reduce，避免中间分配）。
    fn collect_parallel_instances(
        &self,
        visible_tick_end: f32,
        candidate_count: usize,
        default_color: [f32; 4],
        hover_color: [f32; 4],
        active_color: [f32; 4],
        selected_color: [f32; 4],
    ) -> Vec<NoteInstance> {
        let cache = self.spatial.query_cache.borrow();
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);

        // 预收集需要在线程安全的结构中访问的数据
        // 收集 (index, tick, key, length) 元组，避免在并行闭包中访问 self.editor_state.data.notes
        let note_data: Vec<(usize, f32, u16, f32)> = cache
            .iter()
            .filter_map(|&i| {
                self.editor_state
                    .data
                    .notes
                    .get(i)
                    .map(|note| (i, note.tick, note.key, note.length))
            })
            .collect();

        // 预收集选中状态（避免在闭包中访问 HashSet）
        let selected_indices: Vec<bool> = note_data
            .iter()
            .map(|&(i, _, _, _)| self.editor_state.interaction.selected_notes.contains(&i))
            .collect();

        // 并行处理：使用 fold + reduce 模式，避免中间内存分配
        let edit_state_copy = self.editor_state.interaction.edit_state.clone();
        let hover_state_copy = self.editor_state.interaction.hover_state;
        // 延迟提交方案：clone pending_drag_state（None 时只复制 discriminant，开销极小）
        let pending_drag_copy = self.pending_drag_state.clone();

        note_data
            .into_par_iter()
            .enumerate()
            .filter_map(|(idx, (i, tick, key, length))| {
                // 过滤 tick 范围（用原始 tick，ghost 偏移不影响视口过滤）
                if tick > visible_tick_end {
                    return None;
                }

                let is_selected = selected_indices.get(idx).copied().unwrap_or(false);

                // 合并 pending_drag_state 与当前 drag_state 的 delta，计算 ghost 位置
                let delta = crate::rendering::ghost_delta_for_index(
                    i,
                    &pending_drag_copy,
                    &edit_state_copy,
                );
                let (render_tick, render_key, color_arr) = if let Some((dt, dk)) = delta {
                    let gt = (tick + dt as f32).max(0.0);
                    let gk = (key as i32 + dk as i32).clamp(0, max_key as i32) as u16;
                    (gt, gk, active_color)
                } else {
                    match &edit_state_copy {
                        EditState::ResizingStart { note_index, .. }
                        | EditState::ResizingEnd { note_index, .. }
                            if *note_index == i =>
                        {
                            (tick, key, active_color)
                        }
                        _ if is_selected => (tick, key, selected_color),
                        EditState::Idle if hover_state_copy.is_some_and(|(idx, _)| idx == i) => {
                            (tick, key, hover_color)
                        }
                        _ => (tick, key, default_color),
                    }
                };

                Some(NoteInstance::new(
                    render_tick,
                    render_key as f32,
                    length,
                    color_arr,
                ))
            })
            .fold(
                || Vec::with_capacity(candidate_count / rayon::current_num_threads() + 1),
                |mut local_instances, instance| {
                    local_instances.push(instance);
                    local_instances
                },
            )
            .reduce(Vec::new, |mut a, b| {
                a.extend(b);
                a
            })
    }

    /// 小数据量路径：顺序收集可见音符实例，避免并行开销。
    fn collect_sequential_instances(
        &self,
        visible_tick_end: f32,
        candidate_count: usize,
        default_color: [f32; 4],
        hover_color: [f32; 4],
        active_color: [f32; 4],
        selected_color: [f32; 4],
    ) -> Vec<NoteInstance> {
        let mut result = Vec::with_capacity(candidate_count);
        let cache = self.spatial.query_cache.borrow();
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        // 延迟提交方案：clone pending_drag_state（None 时开销极小）
        let pending_drag_copy = self.pending_drag_state.clone();
        for &i in cache.iter() {
            if let Some(note) = self.editor_state.data.notes.get(i) {
                if note.tick > visible_tick_end {
                    continue;
                }

                let tick = note.tick;
                let key = note.key;
                let length = note.length;
                let is_selected = self.editor_state.interaction.selected_notes.contains(&i);

                // 合并 pending_drag_state 与当前 drag_state 的 delta，计算 ghost 位置
                let delta = crate::rendering::ghost_delta_for_index(
                    i,
                    &pending_drag_copy,
                    &self.editor_state.interaction.edit_state,
                );
                let (render_tick, render_key, color_arr) = if let Some((dt, dk)) = delta {
                    let gt = (tick + dt as f32).max(0.0);
                    let gk = (key as i32 + dk as i32).clamp(0, max_key as i32) as u16;
                    (gt, gk, active_color)
                } else {
                    match &self.editor_state.interaction.edit_state {
                        EditState::ResizingStart { note_index, .. }
                        | EditState::ResizingEnd { note_index, .. }
                            if *note_index == i =>
                        {
                            (tick, key, active_color)
                        }
                        _ if is_selected => (tick, key, selected_color),
                        EditState::Idle
                            if self
                                .editor_state
                                .interaction
                                .hover_state
                                .is_some_and(|(idx, _)| idx == i) =>
                        {
                            (tick, key, hover_color)
                        }
                        _ => (tick, key, default_color),
                    }
                };

                result.push(NoteInstance::new(
                    render_tick,
                    render_key as f32,
                    length,
                    color_arr,
                ));
            }
        }
        result
    }

    /// 渲染正在绘制的音符与（铅笔工具空闲时的）预览音符。
    fn push_drawing_and_preview_instances(
        &self,
        instances: &mut Vec<NoteInstance>,
        default_color: [f32; 4],
        active_color: [f32; 4],
    ) {
        let es = &self.editor_state;
        let view = &es.view;
        let canvas = &es.canvas;
        let interaction = &es.interaction;

        // 渲染正在绘制的音符
        if let EditState::Drawing {
            start_tick,
            key,
            current_tick,
        } = interaction.edit_state
        {
            let (tick, length) = if current_tick > start_tick {
                (start_tick, current_tick - start_tick)
            } else if current_tick < start_tick {
                (current_tick, start_tick - current_tick)
            } else {
                (start_tick, view.default_note_length)
            };
            let length = length.max(view.snap_precision);

            instances.push(NoteInstance::new(tick, key as f32, length, active_color));
        } else if let Some(pos) = canvas.cursor_position {
            // 预览音符 - 当处于空闲状态且没有悬停音符时，使用铅笔工具时显示
            if interaction.edit_state == EditState::Idle
                && interaction.hover_state.is_none()
                && es.tool == Tool::Pencil
            {
                let local_pos = Point::new(pos.0 - canvas.offset_x, pos.1 - canvas.offset_y);
                if self.is_inside_canvas(local_pos) {
                    let tick = self.snap_tick(self.x_to_tick(local_pos.x));
                    let key = self.y_to_key(local_pos.y);

                    let mut preview_color = default_color;
                    preview_color[3] = PREVIEW_NOTE_OPACITY;

                    instances.push(NoteInstance::new(
                        tick,
                        key as f32,
                        view.default_note_length,
                        preview_color,
                    ));
                }
            }
        }
    }
}
