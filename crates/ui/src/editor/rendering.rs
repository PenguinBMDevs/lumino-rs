use iced_core::{Length, Point};
use iced_widget::canvas::Canvas;
use lumino_gfx::NoteInstance;

use crate::constants::editor::PREVIEW_NOTE_OPACITY;
use crate::editor::grid::PianoRollGrid;
use crate::editor::scrollbar_widget;
use crate::toolbar::Tool;
use crate::{Element, Message};

use super::{EditState, Editor};

impl Editor {
    /// 构建编辑器视图
    pub fn view(
        &self,
        on_scroll_x: impl Fn(f32) -> Message + 'static,
        on_scroll_y: impl Fn(f32) -> Message + 'static,
        on_zoom_x: impl Fn(f32, f32) -> Message + 'static,
        on_zoom_y: impl Fn(f32, f32) -> Message + 'static,
    ) -> Element<'_> {
        // 创建带鼠标追踪的 Canvas
        let grid = Canvas::new(PianoRollGrid::new(self))
            .width(Length::Fill)
            .height(Length::Fill);

        let horizontal_scrollbar = scrollbar_widget::ScrollbarWidget::horizontal(
            self.state.scroll_x,
            self.max_scroll_x,
            self.state.zoom_x,
            on_scroll_x,
            on_zoom_x,
        );

        let vertical_scrollbar = scrollbar_widget::ScrollbarWidget::vertical(
            self.state.scroll_y,
            self.max_scroll_y,
            self.state.zoom_y,
            on_scroll_y,
            on_zoom_y,
        );

        let content_with_vscroll = iced_widget::row![grid, vertical_scrollbar];

        iced_widget::column![content_with_vscroll, horizontal_scrollbar].into()
    }

    /// 获取当前需要绘制的音符实例（用于 wgpu 渲染）
    ///
    /// 目前只返回鼠标位置的预览音符，后续可扩展为返回所有 MIDI 音符
    /// 音符只在 Canvas 区域内显示
    pub fn get_note_instances(
        &self,
        theme: &crate::Theme,
        _sidebar_width: f32,
    ) -> Vec<NoteInstance> {
        let mut instances = Vec::new();
        let palette = theme.extended_palette();

        // 默认音符颜色（更弱颜色）
        let default_color = palette.primary.weak.color;
        // 悬停音符颜色
        let hover_color = palette.primary.base.color;
        // 正在绘制/选中的音符颜色（最强颜色）
        let active_color = palette.primary.strong.color;
        // 选中音符的颜色（高亮）
        let selected_color = palette.secondary.strong.color;

        // 计算可见区域（视锥裁剪），避免遍历所有音符
        let view = &self.state;
        let viewport_width = self.canvas_size.x - view.keyboard_width;
        let viewport_height = self.canvas_size.y - view.ruler_height;

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

        if self.note_index_dirty.get() {
            *self.note_index.borrow_mut() = Some(
                crate::editor::spatial_index::NoteSpatialIndex::from_notes(&self.notes),
            );
            self.note_index_dirty.set(false);
            tracing::debug!(
                "Editor: rebuild spatial index for {} notes",
                self.notes.len()
            );
        }

        let candidates = {
            if let Some(index) = &*self.note_index.borrow() {
                // 加一些容差，确保音符尾部也能被包含，虽然树内部已经处理过了
                index.query(
                    visible_tick_start,
                    visible_tick_end,
                    visible_key_min,
                    visible_key_max,
                )
            } else {
                Vec::new()
            }
        };

        // 渲染已放置的音符（带视锥裁剪）
        for &i in &candidates {
            let note = &self.notes[i];
            // CPU 端视锥裁剪：只处理可见范围内的音符
            // 检查 tick 范围（音符结束位置 > 可见起始，且音符起始 < 可见结束）
            if note.tick + note.length < visible_tick_start || note.tick > visible_tick_end {
                continue;
            }
            // 检查 key 范围
            let note_key = note.key;
            if note_key < visible_key_min || note_key > visible_key_max {
                continue;
            }

            let color = match self.edit_state {
                EditState::Dragging { note_index, .. }
                | EditState::ResizingStart { note_index, .. }
                | EditState::ResizingEnd { note_index, .. }
                    if note_index == i =>
                {
                    active_color
                }
                _ if self.selected_notes.contains(&i) => selected_color,
                EditState::Idle if self.hover_state.is_some_and(|(idx, _)| idx == i) => hover_color,
                _ => default_color,
            };

            let instance = note.to_instance(color);
            instances.push(instance);
        }

        // 渲染正在绘制的音符
        if let EditState::Drawing {
            start_tick,
            key,
            current_tick,
        } = self.edit_state
        {
            let (tick, length) = if current_tick > start_tick {
                (start_tick, current_tick - start_tick)
            } else if current_tick < start_tick {
                (current_tick, start_tick - current_tick)
            } else {
                (start_tick, self.state.default_note_length)
            };
            let length = length.max(self.state.snap_precision);
            let drawing_note = super::Note::new(tick, key, length);

            let instance = drawing_note.to_instance(active_color);
            instances.push(instance);
        } else if let Some(pos) = self.cursor_position {
            // 预览音符 - 仅在空闲状态、没有悬停在其他音符上且使用铅笔工具时显示
            if self.edit_state == EditState::Idle
                && self.hover_state.is_none()
                && self.current_tool == Tool::Pencil
            {
                let local_pos =
                    Point::new(pos.x - self.canvas_offset.x, pos.y - self.canvas_offset.y);
                if self.is_inside_canvas(local_pos) {
                    let tick = self.snap_tick(self.x_to_tick(local_pos.x));
                    let key = self.y_to_key(local_pos.y);
                    let preview_note = super::Note::new(tick, key, self.state.default_note_length);

                    let mut preview_color = default_color;
                    preview_color.a = PREVIEW_NOTE_OPACITY;

                    let instance = preview_note.to_instance(preview_color);
                    instances.push(instance);
                }
            }
        }

        instances
    }

    /// 获取框选框的实例（用于渲染选择框）
    pub fn get_selection_box(&self) -> Option<(Point, Point)> {
        if let EditState::Selecting {
            start_pos,
            current_pos,
        } = self.edit_state
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
        // 基本的 Canvas 边界检查
        if local_pos.x < 0.0 || local_pos.x > self.canvas_size.x {
            return false;
        }
        if local_pos.y < 0.0 || local_pos.y > self.canvas_size.y {
            return false;
        }

        // 检查是否在键盘区域外（x 必须大于键盘宽度）
        if local_pos.x < self.state.keyboard_width {
            return false;
        }

        // 检查是否在时间轴标尺下方（y 必须大于标尺高度）
        if local_pos.y < self.state.ruler_height {
            return false;
        }

        true
    }
}
