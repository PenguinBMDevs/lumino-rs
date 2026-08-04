//! 编辑器交互事件处理 — 事件分发主入口
//!
//! 子模块：
//! - pressed:   鼠标按下事件处理
//! - moved:     鼠标移动事件处理
//! - released:  鼠标释放事件处理
//! - edit_ops:  编辑操作入口（占位，实现分散在 clipboard / note_ops / editor 中）

mod edit_ops;
mod moved;
mod pressed;
mod released;

#[cfg(test)]
mod tests;

use super::Editor;
use lumino_ui_core::message::EditorAction;

impl Editor {
    /// 主入口：处理编辑器动作
    pub fn handle_action(&mut self, action: EditorAction) {
        puffin::profile_function!();
        self.editor_state.interaction.pending_audio_actions.clear();

        match action {
            EditorAction::Pressed { pos, shift } => {
                self.handle_pressed(iced_core::Point::new(pos.x, pos.y), shift)
            }
            EditorAction::Moved(pos) => self.handle_moved(iced_core::Point::new(pos.x, pos.y)),
            EditorAction::Released => self.handle_released(),
            EditorAction::Scrolled { delta_x, delta_y } => self.handle_scrolled(delta_x, delta_y),
            EditorAction::DoubleClicked(pos) => {
                self.handle_double_clicked(iced_core::Point::new(pos.x, pos.y))
            }
            EditorAction::DeletePressed => self.handle_delete_pressed(),
            EditorAction::Cut => self.cut_selected_notes(),
            EditorAction::Copy => {
                self.copy_selected_notes();
            }
            EditorAction::Paste => self.paste_notes_from_clipboard(),
            EditorAction::SelectAll => self.select_all_notes(),
            EditorAction::Undo => {
                self.undo();
            }
            EditorAction::Redo => {
                self.redo();
            }
            EditorAction::Scrubbed { tick } => {
                self.playback_position = tick;
                // 固定指示线模式下：更新 fixed_indicator_position 到当前屏幕位置
                // 使指示线出现在点击位置，而不是强制回到预设点
                if self.editor_state.auto_scroll.mode
                    == lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft
                {
                    let v = &self.editor_state.view;
                    let new_pos = (tick * v.zoom_x - v.scroll_x).max(0.0) as u32;
                    self.editor_state.auto_scroll.fixed_indicator_position = new_pos;
                }
            }
            EditorAction::IndicatorDragStart { x } => {
                // 固定指示线模式下拖拽指示线：更新固定位置到鼠标位置
                let new_pos = (x - self.editor_state.view.keyboard_width).max(0.0) as u32;
                self.editor_state.auto_scroll.fixed_indicator_position = new_pos;
                // 同时更新播放位置到鼠标对应的 tick
                let tick = self.x_to_tick(x);
                let snapped_tick = self.snap_tick(tick).max(0.0);
                self.playback_position = snapped_tick;
            }
            EditorAction::IndicatorDragMove { x } => {
                // 拖拽中：持续更新固定位置和播放位置
                let new_pos = (x - self.editor_state.view.keyboard_width).max(0.0) as u32;
                self.editor_state.auto_scroll.fixed_indicator_position = new_pos;
                let tick = self.x_to_tick(x);
                let snapped_tick = self.snap_tick(tick).max(0.0);
                self.playback_position = snapped_tick;
            }
        }
    }

    /// 处理滚动事件（鼠标滚轮 / 触控板）
    /// 使用平滑滚动动画，不直接设置位置。
    ///
    /// 两轴符号约定统一为"增量取反后累加"（内容跟随手指 / 自然滚动方向）：
    /// - 垂直：向上滚（delta_y > 0）→ scroll_y 减小（显示更高音区）；
    /// - 水平：向右滚（delta_x > 0）→ scroll_x 减小（显示更前音符），
    ///   触控板左滑（delta_x < 0）→ scroll_x 增大，内容跟随手指左移。
    ///
    /// 注意：早期实现 X 轴为 `scroll_x + delta_x`（与 Y 轴符号相反），
    /// 导致触控板水平滑动方向反向；现已与 Y 轴统一（力度面板同款约定）。
    /// 两个方向均钳制到有效范围，防止平滑滚动越界。
    pub(crate) fn handle_scrolled(&mut self, delta_x: f32, delta_y: f32) {
        let state = &mut self.editor_state;
        let v = &mut state.view;
        let max_x =
            (state.max_scroll.0 - (state.canvas.size_x - v.keyboard_width).max(0.0)).max(0.0);
        let max_y = (state.max_scroll.1 - (state.canvas.size_y - v.ruler_height).max(0.0)).max(0.0);

        // 以平滑滚动的「当前目标」为基准叠加 delta，而非以 scroll_x/scroll_y 的瞬时值为基准。
        // 原因：触控板斜向滑动时，Windows/winit 会把双指手势拆成两条独立事件
        // （WM_MOUSEWHEEL 仅带 y、WM_MOUSEHWHEEL 仅带 x），iced 各自转为一条 WheelScrolled。
        // 若以瞬时值 scroll_x/scroll_y 为基准，第二条事件会把第一条设好的轴重置回当前位置，
        // 导致斜向滚动退化为单轴。以目标为基准可让两条事件正确累积为对角线滚动。
        let base_x = if v.smooth_scroll.active {
            v.smooth_scroll.target_x
        } else {
            v.scroll_x
        };
        let base_y = if v.smooth_scroll.active {
            v.smooth_scroll.target_y
        } else {
            v.scroll_y
        };
        let target_x = (base_x - delta_x).clamp(0.0, max_x);
        let target_y = (base_y - delta_y).clamp(0.0, max_y);
        v.smooth_scroll.set_target(target_x, target_y);
    }

    /// 处理双击事件
    pub(crate) fn handle_double_clicked(&mut self, pos: iced_core::Point) {
        if self.is_inside_canvas(pos)
            && let Some((index, _)) = self.hit_test_note(pos)
        {
            self.delete_note_by_index(index);
        }
    }

    /// 处理删除键按下事件
    ///
    /// 优先删除悬停音符，若无悬停则删除选中的音符。
    /// 使用 Editor 层方法以触发协作同步事件。
    pub(crate) fn handle_delete_pressed(&mut self) {
        if let Some((index, _)) = self.editor_state.interaction.hover_state {
            self.delete_note_by_index(index);
        } else if self.has_selection() {
            self.delete_selected_notes();
        }
    }
}
