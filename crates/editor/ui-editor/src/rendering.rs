//! 编辑器渲染相关：视口计算、视图构建、可见音符收集
//!
//! 模块拆分：
//! - `ghost`：Ghost 拖动偏移计算辅助函数（hot loop 中调用）
//! - `visible_notes`：视口可见音符收集（渲染热路径，接入 NoteStore）
//! - `tests`：单元测试

mod ghost;
mod ghost_positions;
#[cfg(test)]
mod tests;
mod visible_notes;

pub(crate) use ghost::{copy_deltas_for_index, ghost_delta_for_index, is_copy_ghosted};

use iced_core::{Length, Point};
use iced_widget::Space;
use iced_widget::canvas::Canvas;
use iced_widget::container;

use crate::grid::PianoRollGrid;
use crate::scrollbar_widget;
use crate::{Element, Message};

use super::{EditState, Editor};

impl Editor {
    /// 计算当前视口内可见的音符数据范围
    ///
    /// 返回 `(visible_tick_start, visible_tick_end, visible_key_min, visible_key_max)`。
    /// `overscan_factor` 用于扩展查询范围，例如 0.5 表示每边扩展 50%。
    pub fn compute_visible_range(&self, overscan_factor: f32) -> (f32, f32, u16, u16) {
        let es = &self.editor_state;
        let view = &es.view;
        let canvas = &es.canvas;

        let (visible_tick_start, visible_tick_end, visible_key_min, visible_key_max) = if es
            .is_vertical_roll
        {
            // 纵向：tick 垂直（Y），key 水平（X）
            let grid_h = (canvas.size_y - view.keyboard_width).max(0.0);
            let tick_start = (view.scroll_x / view.zoom_x).max(0.0);
            let tick_end = ((view.scroll_x + grid_h) / view.zoom_x).max(tick_start);
            let key_min_f = (view.scroll_y / view.zoom_y).floor().max(0.0);
            let key_max_f = ((view.scroll_y + canvas.size_x) / view.zoom_y).ceil();
            let key_min = key_min_f as u16;
            let key_max = (key_max_f as u16).saturating_add(1).min(view.key_count);
            (tick_start, tick_end, key_min, key_max)
        } else {
            let viewport_width = canvas.size_x - view.keyboard_width;
            let viewport_height = canvas.size_y - view.ruler_height;

            let tick_start = (view.scroll_x / view.zoom_x).max(0.0);
            let tick_end = ((view.scroll_x + viewport_width) / view.zoom_x).max(tick_start);

            let max_key_index = (view.visible_key_count - 1) as f32;

            // key_top 对应屏幕上方 (Y = ruler_height)，值最大（高音）
            let key_top_f32 = max_key_index - (view.scroll_y / view.zoom_y);
            // key_bottom 对应屏幕下方 (Y = canvas_size.y)，值最小（低音）
            let key_bottom_f32 = max_key_index - ((view.scroll_y + viewport_height) / view.zoom_y);

            let key_max = key_top_f32.ceil() as u16 + 1; // 多取 1 个作为缓冲
            let key_min = (key_bottom_f32.floor().max(0.0) as u16).saturating_sub(1); // 多取 1 个作为缓冲
            (tick_start, tick_end, key_min, key_max)
        };

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

    /// 构建纵向卷帘视图（底部横向钢琴键盘 + 水平时间 / 垂直音高网格）
    ///
    /// 头部对齐键盘顶部（时间从键盘向上递增，向远离键盘方向），Y 向支持上下滚动/缩放。
    /// 底部横向滚动条驱动 `scroll_y` / `zoom_y`（音高轴水平），右侧纵向滚动条驱动
    /// `scroll_x` / `zoom_x`（时间轴垂直，头部在键盘顶部）。
    pub fn view_vertical<'a>(
        &'a self,
        on_scroll_x: impl Fn(f32) -> Message + 'static,
        on_scroll_y: impl Fn(f32) -> Message + 'static,
        on_zoom_x: impl Fn(f32, f32) -> Message + 'static,
        on_zoom_y: impl Fn(f32, f32) -> Message + 'static,
    ) -> Element<'a> {
        let es = &self.editor_state;
        let canvas_width = es.canvas.size_x.max(1.0);
        let canvas_height = es.canvas.size_y.max(1.0);
        // 纵向隐藏横向小节标尺栏：网格占满顶部至键盘顶部，无顶部标尺留白
        let grid_height = (canvas_height - es.view.keyboard_width).max(0.0);

        let grid = Canvas::new(crate::grid::VerticalRollGrid::new(self))
            .width(Length::Fill)
            .height(Length::Fill);

        // 底部横向滚动条：音高轴水平滚动/缩放（scroll_y / zoom_y）
        let keyboard_scrollbar = scrollbar_widget::ScrollbarWidget::horizontal(
            es.view.scroll_y,
            es.max_scroll.1,
            es.view.zoom_y,
            Some(canvas_width),
            on_scroll_y,
            on_zoom_y,
        );

        // 右侧纵向滚动条：时间轴垂直滚动/缩放（scroll_x / zoom_x，头部在键盘顶部）
        // 头部在底部，滚动条需反转：scroll 0（头部在底部）对应滚动条底部，scroll max 对应顶部
        let max_time_scroll = es.max_scroll.0;
        let inverted_scroll_x = (max_time_scroll - es.view.scroll_x).clamp(0.0, max_time_scroll);
        let time_scrollbar = scrollbar_widget::ScrollbarWidget::vertical(
            inverted_scroll_x,
            max_time_scroll,
            es.view.zoom_x,
            Some(grid_height),
            move |x| on_scroll_x((max_time_scroll - x).clamp(0.0, max_time_scroll)),
            move |zoom, fixed_ratio| {
                // 固定比例需反转：滚动条的 fixed_ratio 0 在顶部，1 在底部，而时间轴 0 在底部
                on_zoom_x(zoom, 1.0 - fixed_ratio.clamp(0.0, 1.0))
            },
        );

        let grid_container = iced_widget::container(grid)
            .width(Length::Fill)
            .height(Length::Fill);

        let grid_with_hscroll = iced_widget::column![grid_container, keyboard_scrollbar]
            .width(Length::Fill)
            .height(Length::Fill);

        let content = iced_widget::row![grid_with_hscroll, time_scrollbar]
            .width(Length::Fill)
            .height(Length::Fill);

        // 纵向模式同样支持右键上下文菜单（复用横向组件）
        if self.context_menu.open
            && let Some(position) = self.context_menu.position
        {
            return iced_widget::Stack::new()
                .push(content)
                .push(crate::context_menu::background_close_overlay())
                .push(crate::context_menu::view(position))
                .into();
        }

        // 文字工具编辑态：在文本框位置叠加 TextInput 覆盖层
        if let Some(overlay) = self.text_tool_input_overlay() {
            return iced_widget::Stack::new().push(content).push(overlay).into();
        }

        content.into()
    }

    /// 构建编辑器视图
    pub fn view<'a>(
        &'a self,
        on_scroll_x: impl Fn(f32) -> Message + 'static,
        on_scroll_y: impl Fn(f32) -> Message + 'static,
        on_zoom_x: impl Fn(f32, f32) -> Message + 'static,
        on_zoom_y: impl Fn(f32, f32) -> Message + 'static,
    ) -> Element<'a> {
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

        let editor_content = iced_widget::column![content_with_vscroll, horizontal_scrollbar];

        // 如果右键上下文菜单打开，叠加悬浮面板
        if self.context_menu.open
            && let Some(position) = self.context_menu.position
        {
            return iced_widget::Stack::new()
                .push(editor_content)
                .push(crate::context_menu::background_close_overlay())
                .push(crate::context_menu::view(position))
                .into();
        }

        // 文字工具编辑态：在文本框位置叠加 TextInput 覆盖层
        if let Some(overlay) = self.text_tool_input_overlay() {
            return iced_widget::Stack::new()
                .push(editor_content)
                .push(overlay)
                .into();
        }

        editor_content.into()
    }

    /// 文字工具编辑态的 TextInput 覆盖层（定位在文本框屏幕矩形上）
    ///
    /// 仅当文本框激活且处于编辑态时返回 `Some`；纵向卷帘下 `box_rect_screen`
    /// 返回 `None`，故不叠加。
    #[allow(clippy::type_complexity)]
    fn text_tool_input_overlay<'a>(&'a self) -> Option<Element<'a>> {
        if !(self.editor_state.text_tool.active && self.editor_state.text_tool.editing) {
            return None;
        }
        let (left, top, right, bottom) = crate::grid::text_tool_box::box_rect_screen(self)?;
        let box_w = (right - left).max(1.0);
        let box_h = (bottom - top).max(1.0);
        let input = container(
            iced_widget::text_input("输入文字…", &self.editor_state.text_tool.text)
                .on_input(|s: String| {
                    Message::EditorAction(crate::message::EditorAction::TextToolTextChanged(s))
                })
                .width(Length::Fixed(box_w)),
        )
        .width(Length::Fixed(box_w))
        .height(Length::Fixed(box_h));
        let overlay = iced_widget::column![]
            .push(Space::new().height(Length::Fixed(top)))
            .push(
                iced_widget::row![]
                    .push(Space::new().width(Length::Fixed(left)))
                    .push(input),
            );
        Some(overlay.into())
    }

    /// 获取选择框的屏幕坐标（用于渲染选择框，转置支持）
    ///
    /// 将选择框坐标（tick/key）转换为屏幕坐标，确保选择框与音符对齐
    pub fn get_selection_box(&self) -> Option<(Point, Point)> {
        if let EditState::Selecting {
            start_tick,
            current_tick,
            start_key,
            current_key,
            start_y,
            current_y,
        } = self.editor_state.interaction.edit_state
        {
            if self.editor_state.is_vertical_roll {
                // 纵向：X=key，Y=tick（头部在键盘顶部）
                let view = &self.editor_state.view;
                let start_x = self.key_to_x_vertical(start_key);
                let current_x = self.key_to_x_vertical(current_key) + view.zoom_y;
                // Y 直接用 tick 转置（与 moved.rs 存储的 tick Y 一致，重新计算避免脏数据）
                let start_y_v = self.tick_to_y_vertical(start_tick);
                let current_y_v = self.tick_to_y_vertical(current_tick);
                // 保持与 horizontal 相同的返回语义：start_y/current_y 仍为 Y 值，但此处用计算值
                // 若需兼容旧 stored 值，可忽略 start_y/current_y 直接用计算
                let _ = (start_y, current_y); // 避免未使用警告，纵向不用 stored
                Some((
                    Point::new(start_x.min(current_x), start_y_v.min(current_y_v)),
                    Point::new(start_x.max(current_x), start_y_v.max(current_y_v)),
                ))
            } else {
                let start_x = self.tick_to_x(start_tick);
                let current_x = self.tick_to_x(current_tick);
                Some((
                    Point::new(start_x, start_y),
                    Point::new(current_x, current_y),
                ))
            }
        } else {
            None
        }
    }

    /// 判断是否在 Canvas 有效区域内（纵向转置：键盘在底部，标尺隐藏）
    /// 有效区域 = Canvas 除去键盘区域（纵向底部/横向左侧）、标尺区域（横向顶部）
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

        if es.is_vertical_roll {
            // 纵向：键盘在底部（高度=keyboard_width），无顶部标尺
            let kb_h = es.view.keyboard_width;
            let grid_bottom = es.canvas.size_y - kb_h;
            if local_pos.y > grid_bottom {
                return false;
            }
            // X 方向无左侧键盘留白，全宽可用
            true
        } else {
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
}
