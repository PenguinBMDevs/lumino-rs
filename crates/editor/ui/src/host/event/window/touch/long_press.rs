//! 触摸长按 → 模拟右键
//!
//! 触屏没有右键按键：`mouse_area::on_right_press` 一类的菜单（音轨右键菜单、
//! 素材右键菜单、颜色/数值选择器、钢琴卷帘右键菜单、力度面板自动化菜单）在
//! Android 上**完全没有触发路径** —— `widget/src/mouse_area.rs:407-412` 只认
//! `Mouse(ButtonPressed(Right))`，原生 `Touch` 分支只覆盖左键。
//!
//! 判据与 Android `ViewConfiguration` 对齐：单指按住 500ms 且位移不超过
//! 8 逻辑像素。触发后补一条 `Touch(FingerLost)` 先复位控件按压态（避免
//! 「长按按钮 = 点了它 + 弹右键」），再补左键释放（让画布收尾），最后按下右键。
//!
//! 长按期间没有任何窗口事件，必须靠 iced 已有的 `RedrawRequest::At` 定时线程
//! 唤醒一帧来判定，见 `Host::handle_redraw_request`。

use std::time::{Duration, Instant};

use iced_core::mouse;
use iced_winit::{conversion, winit};

use crate::host::Host;

/// 长按判定时长：与 Android `ViewConfiguration` 的长按 500ms 对齐
const LONG_PRESS_TIMEOUT: Duration = Duration::from_millis(500);

impl Host {
    /// 记录一次可能的长按起点，并安排一次定时唤醒
    pub(super) fn begin_long_press(&mut self, origin: iced_core::Point) {
        let deadline = Instant::now() + LONG_PRESS_TIMEOUT;
        self.touch_press_origin = Some(origin);
        self.touch_long_press_fired = false;
        self.touch_long_press_deadline = Some(deadline);
        self.handle_redraw_request(iced_core::window::RedrawRequest::At(deadline));
    }

    /// 取消**待判定**的长按（多指、拖动、仍有一指按着时都不能触发）
    ///
    /// 刻意不清 `touch_long_press_fired`：已经触发过右键的会话，抬手时仍然
    /// 必须补一次右键释放，否则右键会一直停在按下态。
    pub(super) fn cancel_long_press(&mut self) {
        self.touch_long_press_deadline = None;
    }

    /// 每帧检查长按是否到点（由 `process_pending_events` 在事件早退之前调用）
    ///
    /// `schedule_redraw_at` 只保留**最早**的唤醒时刻，被更早的定时（tooltip）
    /// 顶掉后本判定就再也不会被唤醒，所以每次排查到未到点时都重新登记一次。
    pub(crate) fn poll_touch_long_press(&mut self) {
        let Some(deadline) = self.touch_long_press_deadline else {
            return;
        };

        // 手指数量变化（第二指落下/有一指抬起）时不再算长按
        if self.active_touches.len() != 1 || self.touch_long_press_fired {
            self.touch_long_press_deadline = None;
            return;
        }

        let now = Instant::now();
        if now >= deadline {
            self.fire_long_press();
        } else {
            self.schedule_redraw_at(deadline);
        }
    }

    /// 长按到点：结束左键按下并模拟一次右键按下
    ///
    /// 先补 `Touch(FingerLost)` 再补 `ButtonReleased(Left)`：`FingerLost` 会让
    /// `button` 一类控件复位按压态且**不**发布点击（`button.rs:334-338`），
    /// 避免「长按工具栏按钮 = 点了它 + 弹右键」；随后的 `ButtonReleased`
    /// 则保证画布/编辑器把按下中的编辑收尾。
    fn fire_long_press(&mut self) {
        let Some(finger_id) = self.active_touches.keys().next().copied() else {
            self.touch_long_press_deadline = None;
            return;
        };
        let Some(origin) = self.touch_press_origin else {
            self.touch_long_press_deadline = None;
            return;
        };

        // 结束左键状态（全局兜底 + is_mouse_pressed）
        self.handle_mouse_input_event(
            winit::event::ElementState::Released,
            winit::event::MouseButton::Left,
        );

        // 光标钉回按下点：右键菜单的落点由控件按 `cursor` 命中算出，
        // 长按期间手指可能抖了几像素，直接用当前光标会让菜单偏离目标。
        self.window_ctx.cursor_position = Some(origin);
        self.window_ctx.cursor = mouse::Cursor::Available(origin);

        self.events
            .push(iced_core::Event::Touch(iced_core::touch::Event::FingerLost {
                id: iced_core::touch::Finger(finger_id),
                position: origin,
            }));
        self.events
            .push(iced_core::Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Left,
            )));
        self.events
            .push(iced_core::Event::Mouse(mouse::Event::CursorMoved {
                position: origin,
            }));
        self.events
            .push(iced_core::Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right,
            )));

        self.touch_long_press_fired = true;
        self.touch_long_press_deadline = None;
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 长按已模拟右键后的抬手：只补右键释放
    ///
    /// 不能再走「触摸→左键」合成，否则菜单项会被抬起时多出来的一次左键
    /// 点击命中。
    pub(crate) fn release_touch_right_button(&mut self, touch: &winit::event::Touch) {
        let scale = self.window_ctx.window.scale_factor() as f32;
        let position = conversion::cursor_position(touch.location, scale);
        self.events
            .push(iced_core::Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Right,
            )));
        self.events
            .push(iced_core::Event::Mouse(mouse::Event::CursorMoved {
                position,
            }));
        self.window_ctx.window.request_redraw();
    }
}
