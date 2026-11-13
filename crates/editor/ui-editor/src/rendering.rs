//! 编辑器几何与音符收集（iced 视图层删除后只保留纯数据逻辑）。
//!
//! 模块拆分：
//! - `ghost`：Ghost 拖动偏移计算辅助函数（hot loop 中调用）
//! - `ghost_positions`：拖动位移换算
//! - `visible_notes`：视口可见音符收集（渲染热路径）
//! - `tests`：单元测试

mod ghost;
mod ghost_positions;
#[cfg(test)]
mod tests;
mod visible_notes;

pub(crate) use ghost::{copy_deltas_for_index, ghost_delta_for_index, is_copy_ghosted};

use super::Editor;

/// 编辑器视图上的悬浮层种类（互斥模型；绘制已迁 egui，仅保留仲裁语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
enum ViewOverlay {
    /// 右键上下文菜单（含背景点击关闭层）。
    ContextMenu,
    /// 文字工具编辑态的 TextInput 覆盖层。
    TextToolInput,
}

/// 互斥优先级（纯函数）：右键菜单 > 文字输入覆盖层。
#[cfg(test)]
fn pick_overlay(context_menu: bool, text_tool: bool) -> Option<ViewOverlay> {
    if context_menu {
        Some(ViewOverlay::ContextMenu)
    } else if text_tool {
        Some(ViewOverlay::TextToolInput)
    } else {
        None
    }
}

impl Editor {
    /// 框选框屏幕矩形 `(start, current)`（画布坐标）。
    pub fn get_selection_box(&self) -> Option<(iced_core::Point, iced_core::Point)> {
        use crate::EditState;
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
                let start_y_v = self.tick_to_y_vertical(start_tick);
                let current_y_v = self.tick_to_y_vertical(current_tick);
                let _ = (start_y, current_y); // 纵向不用 stored 值
                Some((
                    iced_core::Point::new(start_x.min(current_x), start_y_v.min(current_y_v)),
                    iced_core::Point::new(start_x.max(current_x), start_y_v.max(current_y_v)),
                ))
            } else {
                let start_x = self.tick_to_x(start_tick);
                let current_x = self.tick_to_x(current_tick);
                Some((
                    iced_core::Point::new(start_x, start_y),
                    iced_core::Point::new(current_x, current_y),
                ))
            }
        } else {
            None
        }
    }

    /// 判断是否在 Canvas 有效区域内（纵向转置：键盘在底部，标尺隐藏）。
    ///
    /// 有效区域 = Canvas 除去键盘区域（纵向底部 / 横向左侧）、标尺区域（横向顶部），
    /// 同时避开窗口边框和菜单栏的边界区域。
    pub fn is_inside_canvas(&self, local_pos: iced_core::Point) -> bool {
        let es = &self.editor_state;

        if local_pos.x < 0.0 || local_pos.x > es.canvas.size_x {
            return false;
        }
        if local_pos.y < 0.0 || local_pos.y > es.canvas.size_y {
            return false;
        }

        if es.is_vertical_roll {
            // 纵向：键盘在底部（高度 = keyboard_width），无顶部标尺
            let kb_h = es.view.keyboard_width;
            let grid_bottom = es.canvas.size_y - kb_h;
            if local_pos.y > grid_bottom {
                return false;
            }
            true
        } else {
            if local_pos.x < es.view.keyboard_width {
                return false;
            }
            if local_pos.y < es.view.ruler_height {
                return false;
            }
            true
        }
    }

    /// 计算当前视口内可见的音符数据范围。
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

            let key_max = key_top_f32.ceil() as u16 + 1; // 多取 1 个作为缓存
            let key_min = (key_bottom_f32.floor().max(0.0) as u16).saturating_sub(1); // 多取 1 个作为缓存
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
}
