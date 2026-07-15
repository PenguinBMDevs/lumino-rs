use iced_core::{Length, Point};
use iced_widget::canvas::Canvas;
use lumino_gfx::NoteInstance;
use rayon::prelude::*;

use lumino_ui_constants::editor::PREVIEW_NOTE_OPACITY;
use crate::grid::PianoRollGrid;
use crate::scrollbar_widget;
use lumino_message::Tool;
use crate::{Element, Message};

use super::{EditState, Editor};

use crate::note::color_to_array;

impl Editor {
    /// 计算当前视口内可见的音符数据范围
    ///
    /// 返回 `(visible_tick_start, visible_tick_end, visible_key_min, visible_key_max)`。
    /// `overscan_factor` 用于扩展查询范围，例如 0.5 表示每边扩展 50%。
    pub fn compute_visible_range(&self, overscan_factor: f32) -> (f32, f32, u16, u16) {
        let es = &self.editor_state;
        let view = &es.view;
        let canvas = &es.canvas;

        let viewport_width = canvas.size_x - view.keyboard_width;
        let viewport_height = canvas.size_y - view.ruler_height;

        let visible_tick_start = (view.scroll_x / view.zoom_x).max(0.0);
        let visible_tick_end =
            ((view.scroll_x + viewport_width) / view.zoom_x).max(visible_tick_start);

        let max_key_index = (view.visible_key_count - 1) as f32;

        // key_top 对应屏幕上方 (Y = ruler_height)，值最大（高音）
        let key_top_f32 = max_key_index - (view.scroll_y / view.zoom_y);
        // key_bottom 对应屏幕下方 (Y = canvas_size.y)，值最小（低音）
        let key_bottom_f32 = max_key_index - ((view.scroll_y + viewport_height) / view.zoom_y);

        let visible_key_max = key_top_f32.ceil() as u16 + 1; // 多取 1 个作为缓冲
        let visible_key_min = (key_bottom_f32.floor().max(0.0) as u16).saturating_sub(1); // 多取 1 个作为缓冲

        if overscan_factor > 0.0 {
            let tick_span = visible_tick_end - visible_tick_start;
            let key_span = visible_key_max.saturating_sub(visible_key_min);
            let tick_expand = tick_span * overscan_factor;
            let key_expand = ((key_span as f32 * overscan_factor) as u16).max(1);

            let expanded_tick_start = (visible_tick_start - tick_expand).max(0.0);
            let expanded_tick_end = visible_tick_end + tick_expand;
            let expanded_key_min = visible_key_min.saturating_sub(key_expand);
            let expanded_key_max = visible_key_max.saturating_add(key_expand);

            return (
                expanded_tick_start,
                expanded_tick_end,
                expanded_key_min,
                expanded_key_max,
            );
        }

        (
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
        )
    }

    /// 收集当前视口内可见的音符数据（tick, key, length）
    ///
    /// `overscan_factor` 用于扩展查询范围，减少频繁重建。0.0 表示精确视口。
    /// 返回可见音符数量，结果写入传入的 buffer。
    ///
    /// 性能优化：
    /// - 仅在音符数据变化时重建空间索引
    /// - 使用空间索引 O(log N) 查询替代全量扫描
    pub fn collect_visible_note_data(
        &self,
        result: &mut Vec<(f32, u16, f32)>,
        overscan_factor: f32,
    ) -> usize {
        result.clear();

        let (visible_tick_start, visible_tick_end, visible_key_min, visible_key_max) =
            self.compute_visible_range(overscan_factor);

        // 重建空间索引（仅当数据变化时）
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
            *self.spatial.note_index.borrow_mut() =
                Some(crate::spatial_index::NoteSpatialIndex::from_note_refs(&note_refs));
            self.spatial.note_index_dirty.set(false);
            tracing::debug!(
                "Editor: rebuild spatial index for {} notes",
                self.editor_state.data.notes.len()
            );
        }

        // 查询可见范围内的音符
        if let Some(index) = &*self.spatial.note_index.borrow() {
            index.collect_instances_in_range(
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                result,
            );
        }

        result.len()
    }

    /// 构建编辑器视图
    pub fn view(
        &self,
        on_scroll_x: impl Fn(f32) -> Message + 'static,
        on_scroll_y: impl Fn(f32) -> Message + 'static,
        on_zoom_x: impl Fn(f32, f32) -> Message + 'static,
        on_zoom_y: impl Fn(f32, f32) -> Message + 'static,
    ) -> Element<'_> {
        // 使用 editor_state 获取视图状态
        let es = &self.editor_state;

        // 创建跟随滚动的 Canvas
        let grid = Canvas::new(PianoRollGrid::new(self))
            .width(Length::Fill)
            .height(Length::Fill);

        let horizontal_scrollbar = scrollbar_widget::ScrollbarWidget::horizontal(
            es.view.scroll_x,
            es.max_scroll.0,
            es.view.zoom_x,
            None,
            on_scroll_x,
            on_zoom_x,
        );

        let viewport_height = (es.canvas.size_y - es.view.ruler_height).max(0.0);
        let vertical_scrollbar = scrollbar_widget::ScrollbarWidget::vertical(
            es.view.scroll_y,
            es.max_scroll.1,
            es.view.zoom_y,
            Some(viewport_height),
            on_scroll_y,
            on_zoom_y,
        );

        let content_with_vscroll = iced_widget::row![grid, vertical_scrollbar];

        iced_widget::column![content_with_vscroll, horizontal_scrollbar].into()
    }

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
        let interaction = &es.interaction;
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
            *self.spatial.note_index.borrow_mut() =
                Some(crate::spatial_index::NoteSpatialIndex::from_note_refs(&note_refs));
            self.spatial.note_index_dirty.set(false);
            tracing::debug!(
                "Editor: rebuild spatial index for {} notes",
                self.editor_state.data.notes.len()
            );
        }

        // 查询可见范围内的音符（每帧渲染时执行，确保视图变化时刷新）
        // 使用线程本地存储的缓存来避免重复分配
        let candidate_count = {
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
        };

        // 预分配内存
        instances.reserve(candidate_count);

        // 直接顺序处理（小数据量）：顺序访问性能更好，且避免内存分配
        // 只有当候选音符数量超过阈值时，才使用并行处理
        const PARALLEL_THRESHOLD: usize = 500;

        if candidate_count >= PARALLEL_THRESHOLD {
            // 大数据量：使用并行处理
            let cache = self.spatial.query_cache.borrow();

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
                .map(|&(i, _, _, _)| interaction.selected_notes.contains(&i))
                .collect();

            // 并行处理：使用 fold + reduce 模式，避免中间内存分配
            let edit_state_copy = interaction.edit_state.clone();
            let hover_state_copy = interaction.hover_state;

            let note_instances: Vec<NoteInstance> = note_data
                .into_par_iter()
                .enumerate()
                .filter_map(|(idx, (i, tick, key, length))| {
                    // 过滤 tick 范围
                    if tick > visible_tick_end {
                        return None;
                    }

                    let is_selected = selected_indices.get(idx).copied().unwrap_or(false);

                    let color_arr = match edit_state_copy {
                        EditState::Dragging { note_index, .. }
                        | EditState::ResizingStart { note_index, .. }
                        | EditState::ResizingEnd { note_index, .. }
                            if note_index == i =>
                        {
                            active_color
                        }
                        _ if is_selected => selected_color,
                        EditState::Idle if hover_state_copy.is_some_and(|(idx, _)| idx == i) => {
                            hover_color
                        }
                        _ => default_color,
                    };

                    Some(NoteInstance::new(tick, key as f32, length, color_arr))
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
                });

            instances.extend(note_instances);
        } else {
            // 小数据量：顺序处理，避免并行开销
            let cache = self.spatial.query_cache.borrow();
            for &i in cache.iter() {
                if let Some(note) = self.editor_state.data.notes.get(i) {
                    if note.tick > visible_tick_end {
                        continue;
                    }

                    let color_arr = match interaction.edit_state {
                        EditState::Dragging { note_index, .. }
                        | EditState::ResizingStart { note_index, .. }
                        | EditState::ResizingEnd { note_index, .. }
                            if note_index == i =>
                        {
                            active_color
                        }
                        _ if interaction.selected_notes.contains(&i) => selected_color,
                        EditState::Idle
                            if interaction.hover_state.is_some_and(|(idx, _)| idx == i) =>
                        {
                            hover_color
                        }
                        _ => default_color,
                    };

                    instances.push(NoteInstance::new(
                        note.tick,
                        note.key as f32,
                        note.length,
                        color_arr,
                    ));
                }
            }
        }

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

    /// 获取选择框的屏幕坐标（用于渲染选择框）
    ///
    /// 将选择框坐标（tick/key）转换为屏幕坐标，确保选择框与音符对齐
    pub fn get_selection_box(&self) -> Option<(Point, Point)> {
        if let EditState::Selecting {
            start_tick,
            start_key,
            current_tick,
            current_key,
        } = self.editor_state.interaction.edit_state
        {
            let start_x = self.tick_to_x(start_tick);
            let start_y = self.key_to_y(start_key);
            let current_x = self.tick_to_x(current_tick);
            let current_y = self.key_to_y(current_key);
            Some((
                Point::new(start_x, start_y),
                Point::new(current_x, current_y),
            ))
        } else {
            None
        }
    }

    /// 判断是否在 Canvas 有效区域内
    /// 有效区域 = Canvas 除去键盘区域（左侧）、标尺区域（顶部）、滚动条区域（底部和右侧）
    /// 同时避开窗口边框和菜单栏的边界区域
    pub fn is_inside_canvas(&self, local_pos: Point) -> bool {
        let es = &self.editor_state;

        // 检查 Canvas 边界
        if local_pos.x < 0.0 || local_pos.x > es.canvas.size_x {
            return false;
        }
        if local_pos.y < 0.0 || local_pos.y > es.canvas.size_y {
            return false;
        }

        // 检查是否在键盘区域外（x 大于键盘宽度）
        if local_pos.x < es.view.keyboard_width {
            return false;
        }

        // 检查是否在标尺区域下方（y 大于标尺高度）
        if local_pos.y < es.view.ruler_height {
            return false;
        }

        true
    }
}
