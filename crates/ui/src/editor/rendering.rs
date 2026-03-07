use iced_core::{Length, Point};
use iced_widget::canvas::Canvas;
use lumino_gfx::NoteInstance;

use crate::{Element, Message};
use crate::constants::dimensions::MENU_SAFE_ZONE;
use crate::constants::editor::PREVIEW_NOTE_OPACITY;
use crate::editor::grid::PianoRollGrid;
use crate::editor::scrollbar_widget;
use crate::toolbar::Tool;

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

        // 渲染已放置的音符
        for (i, note) in self.notes.iter().enumerate() {
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

            let mut instance = note.to_instance(&self.state, color);
            // 转换为窗口坐标：加上 Canvas 偏移
            instance.position[0] += self.canvas_offset.x;
            instance.position[1] += self.canvas_offset.y;
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

            let mut instance = drawing_note.to_instance(&self.state, active_color);
            instance.position[0] += self.canvas_offset.x;
            instance.position[1] += self.canvas_offset.y;
            instances.push(instance);
        } else if let Some(pos) = self.cursor_position {
            // 预览音符 - 仅在空闲状态、没有悬停在其他音符上且使用铅笔工具时显示
            if self.edit_state == EditState::Idle && self.hover_state.is_none() && self.current_tool == Tool::Pencil {
                let local_pos =
                    Point::new(pos.x - self.canvas_offset.x, pos.y - self.canvas_offset.y);
                if self.is_inside_canvas(local_pos) {
                    let tick = self.snap_tick(self.x_to_tick(local_pos.x));
                    let key = self.y_to_key(local_pos.y);
                    let preview_note = super::Note::new(tick, key, self.state.default_note_length);

                    let mut preview_color = default_color;
                    preview_color.a = PREVIEW_NOTE_OPACITY;

                    let mut instance = preview_note.to_instance(&self.state, preview_color);
                    instance.position[0] += self.canvas_offset.x;
                    instance.position[1] += self.canvas_offset.y;
                    instances.push(instance);
                }
            }
        }

        instances
    }

    /// 获取框选框的实例（用于渲染选择框）
    pub fn get_selection_box(
        &self,
    ) -> Option<(Point, Point)> {
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
    /// 有效区域 = Canvas 区域减去键盘区域（左侧）和滚动条区域（底部和右侧）
    /// 同时避开顶部可能被下拉菜单覆盖的区域
    pub(super) fn is_inside_canvas(&self, local_pos: Point) -> bool {
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

        // 避开顶部区域（防止与下拉菜单重叠）
        // 顶部 MENU_SAFE_ZONE 像素区域不渲染音符（给下拉菜单留空间）
        if local_pos.y < MENU_SAFE_ZONE {
            return false;
        }

        true
    }
}
