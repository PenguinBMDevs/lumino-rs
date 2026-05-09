use iced_core::{Length, Point};
use iced_widget::canvas::Canvas;
use lumino_gfx::NoteInstance;
use rayon::prelude::*;

use crate::constants::editor::PREVIEW_NOTE_OPACITY;
use crate::editor::grid::PianoRollGrid;
use crate::editor::scrollbar_widget;
use crate::toolbar::Tool;
use crate::{Element, Message};

use super::{EditState, Editor};

/// 将 iced Color 转换为 [f32; 4] RGBA
#[inline]
fn color_to_array(color: iced_core::Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}

impl Editor {
    /// 构建编辑器视图
    pub fn view(
        &self,
        on_scroll_x: impl Fn(f32) -> Message + 'static,
        on_scroll_y: impl Fn(f32) -> Message + 'static,
        on_zoom_x: impl Fn(f32, f32) -> Message + 'static,
        on_zoom_y: impl Fn(f32, f32) -> Message + 'static,
    ) -> Element<'_> {
        // 使用 editor_state 读取视图状态
        let es = &self.editor_state;

        // 创建带鼠标追踪的 Canvas
        let grid = Canvas::new(PianoRollGrid::new(self))
            .width(Length::Fill)
            .height(Length::Fill);

        let horizontal_scrollbar = scrollbar_widget::ScrollbarWidget::horizontal(
            es.view.scroll_x,
            es.max_scroll.x,
            es.view.zoom_x,
            None,
            on_scroll_x,
            on_zoom_x,
        );

        let viewport_height = (es.canvas.size.y - es.view.ruler_height).max(0.0);
        let vertical_scrollbar = scrollbar_widget::ScrollbarWidget::vertical(
            es.view.scroll_y,
            es.max_scroll.y,
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
    /// 1. 使用 rayon 并行处理音符到 instance 的转换
    /// 2. 预分配容量减少内存分配
    /// 3. 空间索引已经做过裁剪，避免重复检查
    /// 4. 使用线程本地存储的工作缓冲区，避免重复分配
    pub fn update_note_instances(
        &self,
        theme: &crate::Theme,
        _sidebar_width: f32,
        instances: &mut Vec<NoteInstance>,
    ) {
        instances.clear();
        let palette = theme.extended_palette();

        // 默认音符颜色（更弱颜色）
        let default_color = color_to_array(palette.primary.weak.color);
        // 悬停音符颜色
        let hover_color = color_to_array(palette.primary.base.color);
        // 正在绘制/选中的音符颜色（最强颜色）
        let active_color = color_to_array(palette.primary.strong.color);
        // 选中音符的颜色（高亮）
        let selected_color = color_to_array(palette.secondary.strong.color);

        // 使用 editor_state 读取视图/交互/画布状态
        let es = &self.editor_state;
        let view = &es.view;
        let interaction = &es.interaction;
        let canvas = &es.canvas;

        let viewport_width = canvas.size.x - view.keyboard_width;
        let viewport_height = canvas.size.y - view.ruler_height;

        // 计算可见的 tick 范围（使用 scroll_x，不需要 canvas_offset）
        let visible_tick_start = (view.scroll_x / view.zoom_x).max(0.0);
        let visible_tick_end =
            ((view.scroll_x + viewport_width) / view.zoom_x).max(visible_tick_start);

        // 计算可见的 key 范围（使用 scroll_y，考虑 max_key_index）
        let max_key_index = (view.visible_key_count - 1) as f32;

        // key_top 对应屏幕最上方 (Y = ruler_height)，值最大 (最高音)
        let key_top_f32 = max_key_index - (view.scroll_y / view.zoom_y);
        // key_bottom 对应屏幕最下方 (Y = canvas_size.y)，值最小 (最低音)
        let key_bottom_f32 = max_key_index - ((view.scroll_y + viewport_height) / view.zoom_y);

        let visible_key_max = key_top_f32.ceil() as u16 + 1; // 加 1 容错
        let visible_key_min = (key_bottom_f32.floor().max(0.0) as u16).saturating_sub(1); // 减 1 容错

        // 重建空间索引（仅在音符数据变化时）
        if self.note_index_dirty.get() {
            let notes_vec: Vec<_> = self.editor_state.data.notes.iter().cloned().collect();
            *self.note_index.borrow_mut() = Some(
                crate::editor::spatial_index::NoteSpatialIndex::from_notes(&notes_vec),
            );
            self.note_index_dirty.set(false);
            tracing::debug!(
                "Editor: rebuild spatial index for {} notes",
                self.editor_state.data.notes.len()
            );
        }

        // 查询可见范围内的音符（每次渲染都执行，确保滚动/缩放时刷新）
        // 使用线程本地存储的缓冲区，避免重复分配
        let candidate_count = {
            let mut cache = self.query_cache.borrow_mut();
            if let Some(index) = &*self.note_index.borrow() {
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

        // 预分配容量
        instances.reserve(candidate_count);

        // 直接顺序处理（对于小数据量，顺序处理比并行更快，且避免内存分配）
        // 只有当候选数量超过阈值时才使用并行处理
        const PARALLEL_THRESHOLD: usize = 500;

        if candidate_count >= PARALLEL_THRESHOLD {
            // 大数据量：并行处理
            let cache = self.query_cache.borrow();

            // 预收集所有需要的数据到线程安全的结构中
            // 收集 (index, tick, key, length) 的元组，避免在并行闭包中访问 self.editor_state.data.notes
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

            // 预收集选中音符的状态，避免在闭包中访问 HashSet
            let selected_indices: Vec<bool> = note_data
                .iter()
                .map(|&(i, _, _, _)| interaction.selected_notes.contains(&i))
                .collect();

            // 并行处理：使用 fold + reduce 模式，减少内存分配
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
                .reduce(
                    Vec::new,
                    |mut a, b| {
                        a.extend(b);
                        a
                    },
                );

            instances.extend(note_instances);
        } else {
            // 小数据量：顺序处理，避免并行开销
            let cache = self.query_cache.borrow();
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
            // 预览音符 - 仅在空闲状态、没有悬停在其他音符上且使用铅笔工具时显示
            if interaction.edit_state == EditState::Idle
                && interaction.hover_state.is_none()
                && es.tool == Tool::Pencil
            {
                let local_pos = Point::new(pos.x - canvas.offset.x, pos.y - canvas.offset.y);
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

    /// 获取框选框的实例（用于渲染选择框）
    pub fn get_selection_box(&self) -> Option<(Point, Point)> {
        if let EditState::Selecting {
            start_pos,
            current_pos,
        } = self.editor_state.interaction.edit_state
        {
            Some((start_pos, current_pos))
        } else {
            None
        }
    }

    /// 检查点是否在 Canvas 有效区域内
    /// 有效区域 = Canvas 区域减去键盘区域（左侧）、时间轴标尺（顶部）和滚动条区域（底部和右侧）
    /// 同时避开顶部可能被下拉菜单覆盖的区域
    pub fn is_inside_canvas(&self, local_pos: Point) -> bool {
        let es = &self.editor_state;

        // 基本的 Canvas 边界检查
        if local_pos.x < 0.0 || local_pos.x > es.canvas.size.x {
            return false;
        }
        if local_pos.y < 0.0 || local_pos.y > es.canvas.size.y {
            return false;
        }

        // 检查是否在键盘区域外（x 必须大于键盘宽度）
        if local_pos.x < es.view.keyboard_width {
            return false;
        }

        // 检查是否在时间轴标尺下方（y 必须大于标尺高度）
        if local_pos.y < es.view.ruler_height {
            return false;
        }

        true
    }
}
