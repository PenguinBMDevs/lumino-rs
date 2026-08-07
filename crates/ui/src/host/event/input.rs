//! Host 鼠标/触摸输入处理子模块

use iced_core::mouse;
use iced_winit::{conversion, winit};

use crate::host::Host;

impl Host {
    /// 处理光标移动
    pub fn cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        puffin::profile_function!();

        let logical_pos =
            conversion::cursor_position(position, self.render_ctx.viewport.scale_factor());
        if self.window_ctx.cursor_position == Some(logical_pos)
            && !self.window_ctx.is_toolbar_resizing
            && !self.root.sidebar.is_resizing()
            && !self.root.right_sidebar.is_resizing
        {
            return;
        }
        self.window_ctx.cursor = mouse::Cursor::Available(logical_pos);
        // 存储逻辑坐标（与 iced 保持一致）
        self.window_ctx.cursor_position = Some(logical_pos);

        // 如果正在调整工具栏高度，更新工具栏高度
        if self.window_ctx.is_toolbar_resizing {
            self.root.toolbar.update_resize_position(logical_pos.y);
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }

        // 如果正在调整侧边栏宽度，更新侧边栏宽度
        if self.root.sidebar.is_resizing() {
            self.root.sidebar.update_resize_position(logical_pos.x);
            // 同步更新编辑器的画布偏移
            let sidebar_width = self.root.sidebar.width() as f32;
            let current_offset_y = self.root.editor.editor_state.canvas.offset_y;
            self.root
                .editor
                .set_canvas_offset(iced_core::Point::new(sidebar_width, current_offset_y));
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }

        // 如果正在调整右侧栏宽度，更新右侧栏宽度
        // （拖拽方向与左侧相反：手柄在面板左缘，左移增大面板，逻辑在
        //   RightSidebar::update_resize_position 内处理）
        if self.root.right_sidebar.is_resizing {
            self.root
                .right_sidebar
                .update_resize_position(logical_pos.x);
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }
    }

    /// 释放鼠标左键状态（用于拖拽窗口后的状态重置）
    pub fn release_left_mouse_button(&mut self) {
        // 释放鼠标左键
        self.events.push(iced_core::Event::Mouse(
            iced_core::mouse::Event::ButtonReleased(iced_core::mouse::Button::Left),
        ));
        // 同时释放触控状态（如果有的话）
        if let Some(pos) = self.window_ctx.cursor_position {
            self.events.push(iced_core::Event::Touch(
                iced_core::touch::Event::FingerLifted {
                    id: iced_core::touch::Finger(0),
                    position: pos,
                },
            ));
        }
        self.process_pending_events();
    }
}
