//! Host 窗口事件处理和 UI 状态管理子模块

use iced_core::mouse;
use iced_winit::runtime::user_interface;
use iced_winit::{conversion, winit};

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

    fn handle_modifiers_changed_event(&mut self, new_modifiers: &winit::event::Modifiers) {
        let ctrl = super::is_ctrl_or_cmd_pressed(new_modifiers.state());
        self.route_message(message::Message::CtrlKeyChanged(ctrl));
        let shift = new_modifiers
            .state()
            .contains(winit::keyboard::ModifiersState::SHIFT);
        self.route_message(message::Message::ShiftKeyChanged(shift));
    }

    /// 触摸多指回退：用原生 Touch 合成 pinch（无 PinchGesture 平台，如 Windows/Linux 触屏）
    ///
    /// 维护 `active_touches` 映射，第二指刚落（fresh_pinch）不发缩放避免跳变，
    /// 后续根据双指距离比计算 `delta = dist/prev - 1` 走同一 `handle_pinch_gesture`
    /// 返回 `true` 表示已消费（双指捏合中，屏蔽 iced 鼠标合成，避免误拖音符）
    fn handle_touch_for_pinch(&mut self, touch: &winit::event::Touch) -> bool {
        let scale = self.window_ctx.window.scale_factor() as f32;
        let logical = conversion::cursor_position(touch.location, scale);
        match touch.phase {
            winit::event::TouchPhase::Started => {
                self.active_touches.insert(touch.id, logical);
                if self.active_touches.len() != 2 {
                    self.prev_pinch_distance = None;
                }
            }
            winit::event::TouchPhase::Moved => {
                if let Some(p) = self.active_touches.get_mut(&touch.id) {
                    *p = logical;
                } else {
                    self.active_touches.insert(touch.id, logical);
                }
            }
            winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                self.active_touches.remove(&touch.id);
                self.prev_pinch_distance = None;
                return false;
            }
        }

        if self.active_touches.len() == 2 {
            let pts: Vec<iced_core::Point> = self.active_touches.values().copied().collect();
            let dx = pts[0].x - pts[1].x;
            let dy = pts[0].y - pts[1].y;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let center =
                iced_core::Point::new((pts[0].x + pts[1].x) * 0.5, (pts[0].y + pts[1].y) * 0.5);
            // 以双指中点更新光标，确保锚点跟手（对齐 yinhe touch_center）
            self.window_ctx.cursor_position = Some(center);
            self.window_ctx.cursor = iced_core::mouse::Cursor::Available(center);
            if let Some(prev) = self.prev_pinch_distance {
                if prev > 0.0 {
                    let delta = (dist / prev) - 1.0;
                    // 夹到 ±0.15 避免首帧或快速张合跳变（类似 yinhe 0.02 阈值）
                    let clamped = delta.clamp(-0.15, 0.15);
                    if clamped.abs() > 0.01 {
                        self.handle_pinch_gesture(clamped as f64, winit::event::TouchPhase::Moved);
                    }
                }
            }
            self.prev_pinch_distance = Some(dist);
            return true;
        }
        self.prev_pinch_distance = None;
        false
    }

    /// 触摸板捏合缩放（C）：处理 winit `PinchGesture`（macOS 触控板双指捏合）
    ///
    /// `delta` 为 Open 增量（>0 放大），转换为连续 `factor = 1+delta`，锚点跟当前光标
    /// 区域判定对齐 yinhe：键盘区→Y，标尺区→X，网格区→双轴同时缩放
    fn handle_pinch_gesture(&mut self, delta: f64, _phase: winit::event::TouchPhase) {
        // 连续捏合增量很小（~0.01-0.05/帧），夹到 0.85-1.15 避免单帧跳变
        let raw_factor = 1.0 + delta as f32;
        let factor = raw_factor.clamp(0.85, 1.15);
        if (factor - 1.0).abs() < 0.001 {
            return;
        }

        let state = &self.root.editor.editor_state;
        let view = &state.view;
        let canvas = &state.canvas;
        let is_vertical = state.is_vertical_roll;

        // 计算光标在 canvas 内的局部坐标与区域
        let cursor = self.window_ctx.cursor_position;
        let Some(pos) = cursor else {
            // 无光标时以画布中心为锚点，双轴缩放
            let center_ratio = 0.5;
            let new_x = view.zoom_x * factor;
            let new_y = view.zoom_y * factor;
            self.route_message(message::Message::ZoomXChanged {
                zoom: new_x,
                fixed_ratio: center_ratio,
            });
            self.route_message(message::Message::ZoomYChanged {
                zoom: new_y,
                fixed_ratio: center_ratio,
            });
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
            return;
        };

        let local_x = pos.x - canvas.offset_x;
        let local_y = pos.y - canvas.offset_y;

        if is_vertical {
            let kb_h = view.keyboard_width;
            let is_over_kb = local_y >= canvas.size_y - kb_h && canvas.size_y > 0.0;
            if is_over_kb {
                // 键盘区（底部横向）：只缩放音高轴 Y
                let fixed_ratio = (local_x / canvas.size_x.max(1.0)).clamp(0.0, 1.0);
                self.route_message(message::Message::ZoomYChanged {
                    zoom: view.zoom_y * factor,
                    fixed_ratio,
                });
            } else {
                // 网格区：双轴同时缩放（时间 X + 音高 Y），锚点分别为距底比例与水平比例
                let grid_h = (canvas.size_y - kb_h).max(1.0);
                let fixed_ratio_time = ((canvas.size_y - kb_h - local_y) / grid_h).clamp(0.0, 1.0);
                let fixed_ratio_pitch = (local_x / canvas.size_x.max(1.0)).clamp(0.0, 1.0);
                let new_x = view.zoom_x * factor;
                let new_y = view.zoom_y * factor;
                // 先 X 后 Y，保持锚点不动（viewport.rs 内各自 clamp）
                self.route_message(message::Message::ZoomXChanged {
                    zoom: new_x,
                    fixed_ratio: fixed_ratio_time,
                });
                self.route_message(message::Message::ZoomYChanged {
                    zoom: new_y,
                    fixed_ratio: fixed_ratio_pitch,
                });
            }
        } else {
            let is_over_keyboard = local_x < view.keyboard_width;
            let is_over_ruler = local_y < view.ruler_height;
            if is_over_keyboard {
                let viewport_h = (canvas.size_y - view.ruler_height).max(1.0);
                let fixed_ratio = ((local_y - view.ruler_height) / viewport_h).clamp(0.0, 1.0);
                self.route_message(message::Message::ZoomYChanged {
                    zoom: view.zoom_y * factor,
                    fixed_ratio,
                });
            } else if is_over_ruler {
                let viewport_w = (canvas.size_x - view.keyboard_width).max(1.0);
                let fixed_ratio = ((local_x - view.keyboard_width) / viewport_w).clamp(0.0, 1.0);
                self.route_message(message::Message::ZoomXChanged {
                    zoom: view.zoom_x * factor,
                    fixed_ratio,
                });
            } else {
                // 网格区：双轴同时缩放，锚点分别跟鼠标 X/Y（更跟手，类似 yinhe Android 捏合双轴）
                let viewport_w = (canvas.size_x - view.keyboard_width).max(1.0);
                let viewport_h = (canvas.size_y - view.ruler_height).max(1.0);
                let fixed_ratio_x = ((local_x - view.keyboard_width) / viewport_w).clamp(0.0, 1.0);
                let fixed_ratio_y = ((local_y - view.ruler_height) / viewport_h).clamp(0.0, 1.0);
                let new_x = view.zoom_x * factor;
                let new_y = view.zoom_y * factor;
                self.route_message(message::Message::ZoomXChanged {
                    zoom: new_x,
                    fixed_ratio: fixed_ratio_x,
                });
                self.route_message(message::Message::ZoomYChanged {
                    zoom: new_y,
                    fixed_ratio: fixed_ratio_y,
                });
            }
        }

        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
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
            winit::event::WindowEvent::Touch(touch) => {
                // 双指捏合回退：两指时合成 pinch 并消费，三指/单指放行走 iced 鼠标合成
                if self.handle_touch_for_pinch(touch) {
                    return;
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
