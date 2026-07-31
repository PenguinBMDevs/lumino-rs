//! Host 窗口事件处理和 UI 状态管理子模块

use iced_winit::{conversion, winit};
use iced_winit::runtime::user_interface;
use iced_core::mouse;

use crate::host::{Host, types::convert_touch_to_mouse};
use crate::message;

impl Host {
    // ─── handle_events 子方法 ────────────────────────────────────────

    fn handle_resized_event(&mut self) {
        // 最大化/还原时设置保护标志，防止路由被意外切换
        // 后续在 process_frame_preparation 中清除
        self.root.window_resize_guard = true;
        self.route_message(message::Window::maximized(
            self.window_ctx.window.is_maximized(),
        ));
    }

    fn handle_focused_event(&mut self, focused: bool) {
        self.route_message(message::Window::focused(focused));
    }

    fn handle_keyboard_input_event(
        &mut self,
        key_event: &winit::event::KeyEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) {
        use winit::event::ElementState;
        if let (ElementState::Pressed, winit::keyboard::PhysicalKey::Code(code)) =
            (key_event.state, key_event.physical_key)
        {
            self.handle_keyboard_shortcuts(code, modifiers);
        }
    }

    fn handle_mouse_input_event(
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
    }

    fn handle_modifiers_changed_event(&mut self, new_modifiers: &winit::event::Modifiers) {
        let ctrl = super::is_ctrl_or_cmd_pressed(new_modifiers.state());
        self.route_message(message::Message::CtrlKeyChanged(ctrl));
        let shift = new_modifiers
            .state()
            .contains(winit::keyboard::ModifiersState::SHIFT);
        self.route_message(message::Message::ShiftKeyChanged(shift));
    }

    /// 将 winit 窗口事件转换为 iced 事件并加入队列
    fn convert_and_queue_window_event(
        &mut self,
        event: &winit::event::WindowEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) {
        // 提前判断：当前事件是否为 RedrawRequested（避免 conversion 消耗后无法访问）
        let is_redraw_requested = matches!(event, winit::event::WindowEvent::RedrawRequested);

        // 将窗口事件映射到 iced 事件
        if let Some(event) = conversion::window_event(
            event.clone(),
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
        //
        // 避免 RedrawRequested 事件造成的自循环：该事件的唯一来源是上层 `request_redraw()`，
        // 再为此请求重绘会形成 RedrawRequested → handle_events → request_redraw → … 死循环
        if !self.events.is_empty() && !is_redraw_requested {
            self.window_ctx.window.request_redraw();
        }
    }

    // ─── handle_events 主入口 ────────────────────────────────────────

    /// 处理窗口事件
    pub fn handle_events(
        &mut self,
        event: winit::event::WindowEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) {
        use winit::event::WindowEvent::*;

        match &event {
            Resized(_) => self.handle_resized_event(),
            Focused(r) => self.handle_focused_event(*r),
            KeyboardInput { event, .. } => self.handle_keyboard_input_event(event, modifiers),
            MouseInput { state, button, .. } => {
                self.handle_mouse_input_event(*state, *button);
            }
            ModifiersChanged(new_modifiers) => {
                self.handle_modifiers_changed_event(new_modifiers);
            }
            _ => (),
        }

        self.convert_and_queue_window_event(&event, modifiers);
    }

    // ─── 事件队列处理 ────────────────────────────────────────────────

    /// 处理待处理的事件队列
    ///
    /// 此函数在 redraw_requested 中调用，确保同一帧内的多个事件被合并处理。
    ///
    /// ⚠️ GPU 满载根因修复：旧逻辑在 `update_ui_state` 中**无条件**
    /// `request_redraw`，使 `RedrawRequested` 每帧入队 → process_pending_events
    /// 永不早退 → 再次 request_redraw → 自循环（所有走
    /// `DialogManager::update()` 的 dialog 窗口与主窗口都满载）。
    /// 现仅在"状态实际变更"或"iced 返回 State::Updated（需续帧）"时请求重绘，
    /// 与 `render_iced_ui` 既有门控逻辑对齐，斩断自循环。
    pub(crate) fn process_pending_events(&mut self) {
        puffin::profile_function!();

        if self.events.is_empty() {
            return;
        }

        // 构建 UI 并处理事件；同时取回 iced 是否需要下一帧（State::Updated）
        let (messages, is_ui_updated) = self.build_ui_and_process_events();

        // 处理消息并检查状态变更
        let has_state_change = self.handle_event_messages(messages);

        // 仅当状态变更或 iced 需续帧时才请求重绘
        self.update_ui_state(has_state_change || is_ui_updated);
    }

    /// 构建 UI 界面并处理事件，返回产生的消息
    ///
    /// 返回 `(消息列表, iced 是否需要续帧)`：iced 的 `State::Updated`
    /// 表示存在进行中的动画/订阅，需要再次重绘；用于门控 `request_redraw`，
    /// 避免 `RedrawRequested` 反复入队导致自循环。
    fn build_ui_and_process_events(&mut self) -> (Vec<crate::message::Message>, bool) {
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
        let state = {
            puffin::profile_scope!("update_ui");
            interface
                .update(
                    &self.events,
                    self.window_ctx.cursor,
                    &mut self.render_ctx.renderer,
                    &mut self.window_ctx.clipboard,
                    &mut messages,
                )
                .0
        };

        let is_ui_updated = matches!(state, user_interface::State::Updated { .. });

        {
            puffin::profile_scope!("cleanup");
            self.events.clear();
            self.render_ctx.cache = interface.into_cache();
        }

        self.update_cursor_icon(&state);

        (messages, is_ui_updated)
    }

    /// 根据 iced 状态更新光标图标
    fn update_cursor_icon(&mut self, state: &user_interface::State) {
        if let user_interface::State::Updated {
            mouse_interaction, ..
        } = state
        {
            {
                puffin::profile_scope!("cursor_update");
                if let Some(icon) = iced_winit::conversion::mouse_interaction(*mouse_interaction) {
                    self.window_ctx.window.set_cursor(icon);
                    self.window_ctx.window.set_cursor_visible(true);
                } else {
                    self.window_ctx.window.set_cursor_visible(false);
                }
            }
        }
    }

    /// 处理 UI 消息，返回是否有状态变更
    fn handle_event_messages(&mut self, messages: Vec<crate::message::Message>) -> bool {
        puffin::profile_scope!("process_messages");

        let mut has_state_change = false;
        let len = messages.len();
        for (i, message) in messages.into_iter().enumerate() {
            puffin::profile_scope!("msg", format!("msg_{}/{}", i + 1, len));
            if self.process_message(message) {
                has_state_change = true;
            }
        }
        has_state_change
    }

    /// 更新 UI 状态
    ///
    /// ⚠️ GPU 满载根因修复：旧逻辑**无条件** `request_redraw()`，
    /// 使 `RedrawRequested` 每帧入队 → `process_pending_events` 永不早退
    /// → 再次 `request_redraw` → 自循环（所有走 `DialogManager::update()`
    /// 的 dialog 窗口 + 主窗口都满载）。
    /// 现仅在"状态实际变更"或"iced 返回 State::Updated（需续帧）"时请求重绘，
    /// 与 `render_iced_ui` 既有门控逻辑对齐，斩断自循环。
    fn update_ui_state(&mut self, needs_redraw: bool) {
        if needs_redraw {
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }
    }
}
