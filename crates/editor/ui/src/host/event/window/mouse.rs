//! Host 窗口鼠标事件处理子模块 — 鼠标输入与光标图标更新

use iced_winit::runtime::user_interface;
use iced_winit::winit;

use crate::host::Host;

impl Host {
    pub(super) fn handle_mouse_input_event(
        &mut self,
        state: winit::event::ElementState,
        button: winit::event::MouseButton,
    ) {
        use winit::event::ElementState;

        // 更新鼠标按钮状态
        if button == winit::event::MouseButton::Left {
            self.window_ctx.is_mouse_pressed = state == ElementState::Pressed;
        }

        // 全局监听鼠标释放事件，结束工具栏拖拽状态
        if button == winit::event::MouseButton::Left
            && state == ElementState::Released
            && self.window_ctx.is_toolbar_resizing
        {
            self.window_ctx.is_toolbar_resizing = false;
            self.root.toolbar.end_resize();
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }

        // 全局监听鼠标释放事件，结束侧边栏拖拽状态
        if button == winit::event::MouseButton::Left
            && state == ElementState::Released
            && self.root.sidebar.is_resizing()
        {
            self.root.sidebar.end_resize();
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }

        // 全局监听鼠标释放事件，结束右侧栏拖拽状态
        // （拖拽过程中面板变宽、手柄左移，鼠标可能落在面板内容区上，
        //   iced 的 on_release 不再投递给手柄，必须由全局释放兜底收尾）
        if button == winit::event::MouseButton::Left
            && state == ElementState::Released
            && self.root.right_sidebar.is_resizing
        {
            self.root.right_sidebar.end_resize();
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }

        // 全局兜底：画布内按下、画布外释放（力度面板/滚动条/窗口外）时，
        // iced 画布收不到 ButtonReleased，编辑状态（拖动/批量移动/批量复制/
        // 框选/绘制/调整大小）卡死无法收尾——pending 复制/拖动不保存、
        // ghost 卡在屏幕上、后续点击会吞掉未完成的操作。
        // 典型场景：Ctrl+拖动批量复制时把选区向下拖出键盘底部（力度面板区）
        // 松手，副本"表面上放置成功"，但复制从未写入 pending/内存，
        // 滚动后副本消失、内存无数据。
        // handle_released 幂等（Idle 时 noop）：正常画布内释放时兜底先于
        // iced 画布转发执行，画布稍后的 Released 变为 noop，重复执行无害。
        if button == winit::event::MouseButton::Left
            && state == ElementState::Released
            && self.editor_has_incomplete_pointer_edit()
        {
            self.handle_action(crate::message::EditorAction::Released);
            self.ui_dirty = true;
        }
    }

    /// 编辑器是否处于「鼠标按下中」的编辑状态（等待释放收尾）
    ///
    /// 用于全局左键释放兜底：画布内按下后移到画布外释放时，iced 画布
    /// 收不到 ButtonReleased，这些状态会卡死。host 层监听到左键释放时，
    /// 若编辑器仍处于按下中状态，补发 `EditorAction::Released` 完成收尾。
    fn editor_has_incomplete_pointer_edit(&self) -> bool {
        matches!(
            self.root.editor.editor_state.interaction.edit_state,
            crate::editor::EditState::Selecting { .. }
                | crate::editor::EditState::Drawing { .. }
                | crate::editor::EditState::PendingDrag { .. }
                | crate::editor::EditState::Dragging { .. }
                | crate::editor::EditState::DraggingSelection { .. }
                | crate::editor::EditState::DraggingSelectionCopy { .. }
                | crate::editor::EditState::ResizingStart { .. }
                | crate::editor::EditState::ResizingEnd { .. }
                | crate::editor::EditState::ResizingSelectionStart { .. }
                | crate::editor::EditState::ResizingSelectionEnd { .. }
        )
    }

    /// 根据 iced 状态更新光标图标
    pub(super) fn update_cursor_icon(&mut self, state: &user_interface::State) {
        if let user_interface::State::Updated {
            mouse_interaction, ..
        } = state
        {
            self.apply_cursor_interaction(*mouse_interaction);
        }
    }

    /// 应用 iced 的鼠标交互为窗口光标（幂等：仅在变化时真正设置）。
    ///
    /// 每帧无条件调用 `set_cursor` / `set_cursor_visible` 会被系统与 winit 的
    /// 异步设置竞态重置，导致光标在两种形态间闪烁；这里缓存上次应用值，
    /// 仅在图标或可见性变化时调用一次。
    pub(crate) fn apply_cursor_interaction(&mut self, interaction: iced_core::mouse::Interaction) {
        let icon = iced_winit::conversion::mouse_interaction(interaction);
        if self.window_ctx.applied_cursor == Some(icon) {
            return;
        }

        puffin::profile_scope!("cursor_update");
        match icon {
            Some(icon) => {
                self.window_ctx.window.set_cursor(icon);
                self.window_ctx.window.set_cursor_visible(true);
            }
            None => {
                // `Interaction::Hidden`：隐藏光标（`conversion` 返回 None 的唯一情况）。
                self.window_ctx.window.set_cursor_visible(false);
            }
        }
        self.window_ctx.applied_cursor = Some(icon);
    }
}
