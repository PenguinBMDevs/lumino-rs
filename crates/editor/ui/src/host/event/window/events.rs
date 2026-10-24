//! Host 窗口事件处理子模块 — 事件转换、队列处理与 UI 状态门控

use iced_core::mouse;
use iced_core::window::RedrawRequest;
use iced_winit::runtime::user_interface;
use iced_winit::{conversion, winit};

use super::touch::TouchRouting;
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
        if focused {
            // 重新获得焦点后系统可能已重置光标，丢弃缓存以便下一帧重新应用
            self.window_ctx.applied_cursor = None;
        }
        self.route_message(message::Window::focused(focused));
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
            CursorEntered { .. } => {
                // 鼠标重新进入窗口后强制重新应用光标（系统可能已重置）
                self.window_ctx.applied_cursor = None;
            }
            KeyboardInput { event, .. } => self.handle_keyboard_input_event(event, modifiers),
            MouseInput { state, button, .. } => {
                self.handle_mouse_input_event(*state, *button);
            }
            ModifiersChanged(new_modifiers) => {
                self.handle_modifiers_changed_event(new_modifiers);
            }
            winit::event::WindowEvent::Touch(touch) => {
                // 触摸手势层：单指合成指针并同步窗口级按下/抬起；
                // 双指缩放/平移期一律消费（不产生任何指针事件）；
                // 长按已模拟右键时，抬手只补右键释放
                match self.handle_touch(touch) {
                    TouchRouting::Pointer => {}
                    TouchRouting::Consumed => return,
                    TouchRouting::RightRelease => {
                        self.release_touch_right_button(touch);
                        return;
                    }
                }
            }
            PinchGesture { delta, phase, .. } => {
                // 触摸板捏合（C）直接处理，不走 iced conversion（被过滤）
                self.handle_pinch_gesture(*delta, *phase);
                // 捏合已消费，不再进入 iced 队列
                return;
            }
            _ => (),
        }

        // 触摸链路真机诊断（log 门面在 Android 可见）：只对触摸打点，
        // 桌面鼠标不产生额外日志。
        if matches!(event, winit::event::WindowEvent::Touch(_)) {
            let available = self.window_ctx.cursor.position().is_some();
            let pos = self.window_ctx.cursor.position();
            log::debug!(
                "[TOUCH] 排队 iced 事件 cursor_available={available} cursor={pos:?} queue={}",
                self.events.len()
            );
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

        // 触摸长按判定必须排在「事件为空就早退」之前：长按期间没有任何窗口
        // 事件，事件队列是空的，只有定时唤醒这一帧能把它判出来。
        self.poll_touch_long_press();

        if self.events.is_empty() {
            return;
        }

        // 构建 UI 并处理事件；同时取回 iced 是否需要下一帧（State::Updated）
        let (messages, _is_ui_updated) = self.build_ui_and_process_events();

        // 处理消息并检查状态变更
        let has_state_change = self.handle_event_messages(messages);

        // 触摸抬手后的 hover 抑制（纯触屏没有 CursorLeft）：
        // 抬手后 iced 的 cursor 仍停在落点，悬停高亮 / tooltip 延迟 / 音符
        // hover 预览会一直挂着，直到下一次输入。这里把 cursor 置为
        // `Unavailable` 并补一条 `CursorLeft`，让**下一批**事件以「无指针」
        // 重建一次界面（仅触摸路径置位，桌面端永不触发）。
        //
        // 顺序刻意放在本批处理之后：本批还要用真实落点处理 ButtonReleased，
        // 提前置空会让控件按 `cursor.is_over()` 判定失败而丢点击。
        if self.suppress_hover_after_touch {
            self.suppress_hover_after_touch = false;
            self.window_ctx.cursor = iced_core::mouse::Cursor::Unavailable;
            self.events
                .push(iced_core::Event::Mouse(iced_core::mouse::Event::CursorLeft));
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }

        // REND-019（重连后自旋修复）：续帧语义以 iced 的 `redraw_request` 为准——
        // `NextFrame` / `At` 已由 `build_ui_and_process_events` 内的
        // `handle_redraw_request` 驱动（含 ui_dirty 置位）；纯 `Wait` 的
        // `State::Updated` 不得再触发请求。
        // 背景（实测 2026-10-17）：重连后 iced 每帧返回 `Updated{Wait}`，
        // 而 RedrawRequested 会经 `convert_and_queue_window_event` 每帧入队
        // （`is_redraw_requested` 只挡了"再请求"，没挡入队）→ 事件队列恒非空
        // → 每帧全量重建 + `Updated` 再请求 → ~数百 fps 重建/重绘风暴。
        self.update_ui_state(has_state_change);
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
        let redraw_request = match &state {
            user_interface::State::Updated { redraw_request, .. } => *redraw_request,
            user_interface::State::Outdated => RedrawRequest::Wait,
        };

        let is_ui_updated = matches!(state, user_interface::State::Updated { .. });

        {
            puffin::profile_scope!("cleanup");
            self.events.clear();
            self.render_ctx.cache = interface.into_cache();
        }

        self.update_cursor_icon(&state);
        // Tooltip 延迟显示依赖 At 定时唤醒，必须转发 iced 的重绘请求。
        self.handle_redraw_request(redraw_request);

        (messages, is_ui_updated)
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
