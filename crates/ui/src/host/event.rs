//! Host 事件处理子模块 - 处理窗口事件和输入

use iced_winit::{conversion, winit};

use iced_core::mouse;

use crate::host::{Host, types::convert_touch_to_mouse};
use crate::{message, sidebar, toolbar, window};

/// 检查是否按下了 Ctrl 或 Command（macOS）
fn is_ctrl_or_cmd_pressed(modifiers: winit::keyboard::ModifiersState) -> bool {
    modifiers.contains(winit::keyboard::ModifiersState::CONTROL)
        || modifiers.contains(winit::keyboard::ModifiersState::SUPER)
}

impl Host {
    /// 处理键盘快捷键，返回是否有操作
    fn handle_keyboard_shortcuts(
        &mut self,
        key: winit::keyboard::KeyCode,
        modifiers: winit::keyboard::ModifiersState,
    ) {
        let ctrl = is_ctrl_or_cmd_pressed(modifiers);
        let shift = modifiers.contains(winit::keyboard::ModifiersState::SHIFT);

        // 空格键：播放/暂停切换（统一走 toolbar 消息路径）
        if key == winit::keyboard::KeyCode::Space {
            if self.root.toolbar.is_playing {
                self.root
                    .update(message::Message::Toolbar(toolbar::Event::Pause));
            } else {
                self.root
                    .update(message::Message::Toolbar(toolbar::Event::Play));
            }
            self.window_ctx.window.request_redraw();
            return;
        }

        let action = match (key, ctrl, shift) {
            (winit::keyboard::KeyCode::Delete | winit::keyboard::KeyCode::Backspace, ..) => {
                Some(message::EditorAction::DeletePressed)
            }
            (winit::keyboard::KeyCode::KeyZ, true, false) => Some(message::EditorAction::Undo),
            (winit::keyboard::KeyCode::KeyZ, true, true)
            | (winit::keyboard::KeyCode::KeyY, true, _) => Some(message::EditorAction::Redo),
            (winit::keyboard::KeyCode::KeyX, true, _) => Some(message::EditorAction::Cut),
            (winit::keyboard::KeyCode::KeyC, true, _) => Some(message::EditorAction::Copy),
            (winit::keyboard::KeyCode::KeyV, true, _) => Some(message::EditorAction::Paste),
            (winit::keyboard::KeyCode::KeyA, true, _) => Some(message::EditorAction::SelectAll),
            _ => None,
        };

        if let Some(action) = action {
            self.root.handle_editor_action(action);
            // 仅请求重绘，不重建UI树（编辑器操作由canvas/WGPU层处理）
            self.window_ctx.window.request_redraw();
        }
    }

    /// 处理光标移动
    pub fn cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        puffin::profile_function!();

        let logical_pos =
            conversion::cursor_position(position, self.render_ctx.viewport.scale_factor());
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
            let current_offset = self.root.editor.editor_state.canvas.offset;
            self.root
                .editor
                .set_canvas_offset(iced_core::Point::new(sidebar_width, current_offset.y));
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }
    }

    /// 处理窗口事件
    pub fn handle_events(
        &mut self,
        event: winit::event::WindowEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) {
        use winit::event::ElementState;
        use winit::event::WindowEvent::*;

        match &event {
            Resized(_) => {
                self.root.update(message::Window::maximized(
                    self.window_ctx.window.is_maximized(),
                ));
            }
            Focused(r) => {
                self.root.update(message::Window::focused(*r));
            }
            KeyboardInput { event, .. } => {
                // 处理键盘事件
                if let (ElementState::Pressed, winit::keyboard::PhysicalKey::Code(code)) =
                    (event.state, event.physical_key)
                {
                    self.handle_keyboard_shortcuts(code, modifiers);
                }
            }
            MouseInput { state, button, .. } => {
                // 更新鼠标按钮状态
                if *button == winit::event::MouseButton::Left {
                    self.window_ctx.is_mouse_pressed = *state == ElementState::Pressed;
                }

                // 全局监听鼠标释放事件，结束工具栏拖拽状态
                if *button == winit::event::MouseButton::Left
                    && *state == ElementState::Released
                    && self.window_ctx.is_toolbar_resizing
                {
                    self.window_ctx.is_toolbar_resizing = false;
                    self.root.toolbar.end_resize();
                    self.ui_dirty = true;
                    self.window_ctx.window.request_redraw();
                }

                // 全局监听鼠标释放事件，结束侧边栏拖拽状态
                if *button == winit::event::MouseButton::Left
                    && *state == ElementState::Released
                    && self.root.sidebar.is_resizing()
                {
                    self.root.sidebar.end_resize();
                    self.ui_dirty = true;
                    self.window_ctx.window.request_redraw();
                }
            }
            _ => (),
        }

        // 将窗口事件映射到 iced 事件
        if let Some(event) = conversion::window_event(
            event,
            self.window_ctx.window.scale_factor() as f32,
            modifiers,
        ) {
            let converted_events = convert_touch_to_mouse(event);

            // 事件合并：如果新事件是 CursorMoved，且队列最后一个也是 CursorMoved，则替换
            for event in converted_events {
                if let iced_core::Event::Mouse(mouse::Event::CursorMoved { .. }) = &event {
                    // 检查队列最后一个事件是否也是 CursorMoved
                    if let Some(last) = self.events.last()
                        && matches!(
                            last,
                            iced_core::Event::Mouse(mouse::Event::CursorMoved { .. })
                        )
                    {
                        // 替换最后一个事件
                        self.events.pop();
                    }
                }
                self.events.push(event);
            }
        }

        // 注意：事件处理推迟到 redraw_requested 中统一处理
        // 这样可以合并同一帧内的多个事件，减少 UI 重建次数
        // 但如果有事件需要处理，必须请求重绘以确保事件被及时处理
        if !self.events.is_empty() {
            self.window_ctx.window.request_redraw();
        }
    }

    /// 处理待处理的事件队列
    ///
    /// 此函数在 redraw_requested 中调用，确保同一帧内的多个事件被合并处理
    pub(crate) fn process_pending_events(&mut self) {
        puffin::profile_function!();

        if self.events.is_empty() {
            return;
        }

        // 构建 UI 并处理事件
        let messages = self.build_ui_and_process_events();

        // 处理消息并检查状态变更
        let has_state_change = self.handle_event_messages(messages);

        // 更新 UI 状态
        self.update_ui_state(has_state_change);
    }

    /// 构建 UI 界面并处理事件，返回产生的消息
    fn build_ui_and_process_events(&mut self) -> Vec<crate::message::Message> {
        // 临时取出缓存以避免借用冲突
        let cache = std::mem::take(&mut self.render_ctx.cache);

        let mut interface = {
            puffin::profile_scope!("build_ui");
            iced_winit::runtime::user_interface::UserInterface::build(
                self.root.view(),
                self.render_ctx.viewport.logical_size(),
                cache,
                &mut self.render_ctx.renderer,
            )
        };

        let mut messages = Vec::new();

        {
            puffin::profile_scope!("update_ui");
            let _ = interface.update(
                &self.events,
                self.window_ctx.cursor,
                &mut self.render_ctx.renderer,
                &mut self.window_ctx.clipboard,
                &mut messages,
            );
        }

        self.events.clear();
        self.render_ctx.cache = interface.into_cache();

        messages
    }

    /// 处理 UI 消息，返回是否有状态变更
    fn handle_event_messages(&mut self, messages: Vec<crate::message::Message>) -> bool {
        puffin::profile_scope!("process_messages");

        let mut has_state_change = false;
        for message in messages {
            if self.process_message(message) {
                has_state_change = true;
            }
        }
        has_state_change
    }

    /// 更新 UI 状态
    fn update_ui_state(&mut self, has_state_change: bool) {
        if has_state_change {
            self.ui_dirty = true;
        }
        self.window_ctx.window.request_redraw();
    }

    /// 处理单个消息，返回是否有状态变更
    pub(crate) fn process_message(&mut self, message: message::Message) -> bool {
        // 处理窗口动作消息
        match &message {
            message::Message::Window(window::Event::TrafficAction(action)) => {
                self.window_ctx.pending_window_action = Some(action.clone());
                return false; // 窗口动作不需要 UI 重建
            }
            message::Message::Window(window::Event::ToggleMaximize) => {
                self.window_ctx.pending_window_action = Some(window::TrafficAction::ToggleMaximize);
                return false;
            }
            message::Message::Window(window::Event::Close) => {
                self.window_ctx.pending_window_action = Some(window::TrafficAction::Close);
                return false;
            }
            message::Message::Window(window::Event::Drag) => {
                self.window_ctx.pending_drag = true;
                return false;
            }
            // 处理工具栏调整大小事件
            message::Message::Toolbar(toolbar::Event::ResizeDragStarted(_)) => {
                if let Some(pos) = self.window_ctx.cursor_position {
                    self.window_ctx.is_toolbar_resizing = true;
                    self.root.toolbar.start_resize(pos.y);
                }
                return true; // 工具栏大小改变需要 UI 重建
            }
            message::Message::Toolbar(toolbar::Event::ResizeDragEnded) => {
                self.window_ctx.is_toolbar_resizing = false;
                self.root.toolbar.end_resize();
                return true;
            }
            // 处理侧边栏调整大小事件
            message::Message::Sidebar(sidebar::Event::ResizeDragStarted(_)) => {
                if let Some(pos) = self.window_ctx.cursor_position {
                    self.root.sidebar.start_resize(pos.x);
                }
                return true; // 侧边栏大小改变需要 UI 重建
            }
            message::Message::Sidebar(sidebar::Event::ResizeDragEnded) => {
                self.root.sidebar.end_resize();
                return true;
            }
            _ => {}
        }

        // 主题变更：需要同时失效 wgpu 网格/音符缓存以刷新颜色
        if matches!(&message, message::Message::Window(window::Event::Theme(_))) {
            self.root.update(message);
            self.root.editor.keyboard_cache.clear();
            self.root.editor.ruler_cache.clear();
            self.render_ctx.render_cache.grid_viewport_hash = 0;
            self.render_ctx.render_cache.note_viewport_hash = 0;
            self.root.editor.grid_cache.clear();
            return true;
        }

        // 其他消息交给 root 处理，假设可能有状态变更
        self.root.update(message);
        true
    }

    /// 获取并清除待处理的窗口动作
    pub fn take_window_action(&mut self) -> Option<window::TrafficAction> {
        self.window_ctx.pending_window_action.take()
    }

    /// 获取并清除待处理的拖动标记
    pub fn take_drag(&mut self) -> bool {
        let drag = self.window_ctx.pending_drag;
        self.window_ctx.pending_drag = false;
        drag
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
