//! Host 窗口事件处理子模块 — 事件转换、队列处理与 UI 状态门控

use winit;

use super::touch::TouchRouting;
use crate::host::Host;
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
        } else {
            // 失焦时 IME 状态可能收不到 Disabled（系统直接切走焦点），主动复位，
            // 否则回到窗口后裸字母快捷键会一直失效。
            self.ime_active = false;
            // 失焦时 Shift 的 Released 也可能收不到：主工具与工具栏都不能残留约束态。
            self.root.editor.set_shift_pressed(false);
            self.root.toolbar.shift_pressed = false;
            self.dispatch_canvas_focus_lost();
        }
        self.route_message(message::Window::focused(focused));
    }

    // ─── handle_events 主入口 ────────────────────────────────────────

    /// 处理窗口事件
    pub fn handle_events(
        &mut self,
        event: winit::event::WindowEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) {
        use winit::event::WindowEvent::*;

        // 先把指针 / 滚轮 / 修饰键 / 焦点同步给 egui 叠加层（悬停高亮、
        // `ScrollArea` 滚动、菜单点击依赖它们），随后仍走 iced 的事件转换。
        // 命中 egui 菜单占用区时，本次事件只给 egui（见下方提前返回）。
        let consumed_by_egui = self.capture_egui_overlay_input(&event);

        match &event {
            Resized(_) => self.handle_resized_event(),
            Focused(r) => self.handle_focused_event(*r),
            CursorEntered { .. } => {
                // 鼠标重新进入窗口后强制重新应用光标（系统可能已重置）
                self.window_ctx.applied_cursor = None;
            }
            KeyboardInput { event, .. } => {
                // egui 正在编辑文本（输入框 / 文本框）时，按键只给 egui：
                // 否则裸字母会切工具（B/C/E/S…）、Ctrl+C 会复制音符，输入框直接失控。
                if !consumed_by_egui {
                    self.handle_keyboard_input_event(event, modifiers);
                }
            }
            Ime(ime) => {
                // iced text_input 获得焦点时会 set_ime_allowed(true)，winit 随之发
                // Ime::Enabled；失焦发 Disabled。裸字母快捷键据此让路。
                match ime {
                    winit::event::Ime::Enabled => self.ime_active = true,
                    winit::event::Ime::Disabled => self.ime_active = false,
                    _ => {}
                }
            }
            MouseInput { state, button, .. } => {
                self.handle_mouse_input_event(*state, *button);
            }
            MouseWheel { delta, .. } => {
                use winit::event::MouseScrollDelta;
                let (dx, dy, precise) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (*x, *y, false),
                    MouseScrollDelta::PixelDelta(position) => {
                        (position.x as f32, position.y as f32, true)
                    }
                };
                self.dispatch_canvas_wheel(dx, dy, precise);
            }
            ModifiersChanged(new_modifiers) => {
                self.handle_modifiers_changed_event(new_modifiers);
                self.dispatch_canvas_modifiers(new_modifiers.state());
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
            log::debug!("[TOUCH] 事件已处理 cursor_available={available} cursor={pos:?}");
        }

        if consumed_by_egui {
            // 菜单接管了这次点击/滚轮：只请求重绘（egui 悬停态需要更新）。
            self.window_ctx.window.request_redraw();
        }
    }

    // ─── 事件队列处理 ────────────────────────────────────────────────

    /// 处理待处理的事件队列
    ///
    /// iced widget 树**不再构建**（egui 已是唯一的绘制与输入通道）：窗口事件只经
    /// `capture_egui_overlay_input`（egui）与 winit 直连（画布 / 快捷键 / 触摸手势）
    /// 消费，本队列只作为历史缓冲，逐帧清空。触摸长按判定仍需每帧轮询。
    pub(crate) fn process_pending_events(&mut self) {
        puffin::profile_function!();

        // 触摸长按判定必须排在清空之前：长按期间没有任何窗口事件，事件队列是空的，
        // 只有定时唤醒这一帧能把它判出来。
        self.poll_touch_long_press();

        self.events.clear();
    }
}
