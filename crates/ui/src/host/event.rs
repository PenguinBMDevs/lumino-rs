//! Host 事件处理子模块 - 处理窗口事件和输入

use iced_wgpu::graphics::Viewport;
use iced_winit::{Clipboard, conversion, winit};

use iced_core::{Event, Size, mouse};

use crate::host::{Host, types::convert_touch_to_mouse};
use crate::{message, toolbar, window};

impl Host {
    /// 处理光标移动
    pub fn cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        let logical_pos = conversion::cursor_position(position, self.viewport.scale_factor());
        self.cursor = mouse::Cursor::Available(logical_pos);
        // 存储逻辑坐标（与 iced 保持一致）
        self.cursor_position = Some(logical_pos);

        tracing::debug!(
            "光标移动：物理位置=({:?}), 逻辑位置=({}, {})",
            position,
            logical_pos.x,
            logical_pos.y
        );

        // 如果正在调整工具栏高度，更新工具栏高度
        if self.is_toolbar_resizing {
            self.root.toolbar.update_resize_position(logical_pos.y);
            self.clear_cache();
            self.window.request_redraw();
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
        use winit::keyboard::{KeyCode, PhysicalKey};

        match &event {
            Resized(_) => {
                self.root
                    .update(message::Window::maximized(self.window.is_maximized()));
            }
            Focused(r) => {
                self.root.update(message::Window::focused(*r));
            }
            KeyboardInput { event, .. } => {
                // 处理键盘事件
                if event.state == ElementState::Pressed {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Delete)
                        | PhysicalKey::Code(KeyCode::Backspace) => {
                            // 发送删除音符的消息
                            self.root
                                .editor
                                .handle_action(message::EditorAction::DeletePressed);
                            self.window.request_redraw();
                        }
                        PhysicalKey::Code(KeyCode::KeyZ) => {
                            // Ctrl+Z: 撤销, Ctrl+Shift+Z: 重做
                            let ctrl_or_cmd = modifiers
                                .contains(winit::keyboard::ModifiersState::CONTROL)
                                || modifiers.contains(winit::keyboard::ModifiersState::SUPER);
                            if ctrl_or_cmd {
                                if modifiers.contains(winit::keyboard::ModifiersState::SHIFT) {
                                    // Ctrl+Shift+Z: 重做
                                    self.root.editor.handle_action(message::EditorAction::Redo);
                                } else {
                                    // Ctrl+Z: 撤销
                                    self.root.editor.handle_action(message::EditorAction::Undo);
                                }
                                self.window.request_redraw();
                            }
                        }
                        PhysicalKey::Code(KeyCode::KeyY) => {
                            // Ctrl+Y: 重做
                            if modifiers.contains(winit::keyboard::ModifiersState::CONTROL)
                                || modifiers.contains(winit::keyboard::ModifiersState::SUPER)
                            {
                                self.root.editor.handle_action(message::EditorAction::Redo);
                                self.window.request_redraw();
                            }
                        }
                        _ => {}
                    }
                }
            }
            MouseInput { state, button, .. } => {
                // 全局监听鼠标释放事件，结束工具栏拖拽状态
                if *button == winit::event::MouseButton::Left
                    && *state == ElementState::Released
                    && self.is_toolbar_resizing
                {
                    self.is_toolbar_resizing = false;
                    self.root.toolbar.end_resize();
                    // 清除缓存以强制重绘
                    self.clear_cache();
                    self.window.request_redraw();
                }
            }
            _ => (),
        }

        // 将窗口事件映射到 iced 事件
        if let Some(event) =
            conversion::window_event(event, self.window.scale_factor() as f32, modifiers)
        {
            let converted_events = convert_touch_to_mouse(event);
            self.events.extend(converted_events);
        }

        // 处理事件
        self.process_pending_events();
    }

    /// 处理待处理的事件队列
    fn process_pending_events(&mut self) {
        if self.events.is_empty() {
            return;
        }

        // 临时取出缓存以避免借用冲突
        let cache = std::mem::take(&mut self.cache);

        let mut interface = iced_winit::runtime::user_interface::UserInterface::build(
            self.root.view(),
            self.viewport.logical_size(),
            cache,
            &mut self.renderer,
        );

        let mut messages = Vec::new();

        let _ = interface.update(
            &self.events,
            self.cursor,
            &mut self.renderer,
            &mut self.clipboard,
            &mut messages,
        );

        self.events.clear();
        self.cache = interface.into_cache();

        // 应用消息
        for message in messages {
            self.process_message(message);
        }

        // 清除缓存以确保界面重新构建（特别是侧边栏切换后）
        self.clear_cache();

        self.window.request_redraw();
    }

    /// 处理单个消息
    fn process_message(&mut self, message: message::Message) {
        // 处理窗口动作消息
        match &message {
            message::Message::Window(window::Event::TrafficAction(action)) => {
                self.pending_window_action = Some(action.clone());
            }
            message::Message::Window(window::Event::ToggleMaximize) => {
                self.pending_window_action = Some(window::TrafficAction::ToggleMaximize);
            }
            message::Message::Window(window::Event::Close) => {
                self.pending_window_action = Some(window::TrafficAction::Close);
            }
            message::Message::Window(window::Event::Drag) => {
                self.pending_drag = true;
            }
            // 处理工具栏调整大小事件
            message::Message::Toolbar(toolbar::Event::ResizeDragStarted(_)) => {
                if let Some(pos) = self.cursor_position {
                    self.is_toolbar_resizing = true;
                    self.root.toolbar.start_resize(pos.y);
                }
            }
            message::Message::Toolbar(toolbar::Event::ResizeDragEnded) => {
                self.is_toolbar_resizing = false;
                self.root.toolbar.end_resize();
            }
            _ => {}
        }

        self.root.update(message);
    }

    /// 获取并清除待处理的窗口动作
    pub fn take_window_action(&mut self) -> Option<window::TrafficAction> {
        self.pending_window_action.take()
    }

    /// 获取并清除待处理的拖动标记
    pub fn take_drag(&mut self) -> bool {
        let drag = self.pending_drag;
        self.pending_drag = false;
        drag
    }

    /// 释放鼠标左键状态（用于拖拽窗口后的状态重置）
    pub fn release_left_mouse_button(&mut self) {
        // 释放鼠标左键
        self.events.push(iced_core::Event::Mouse(
            iced_core::mouse::Event::ButtonReleased(iced_core::mouse::Button::Left),
        ));
        // 同时释放触控状态（如果有的话）
        if let Some(pos) = self.cursor_position {
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
