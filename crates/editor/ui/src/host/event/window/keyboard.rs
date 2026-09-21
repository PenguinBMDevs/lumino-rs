//! Host 窗口键盘/修饰键/触摸捏合缩放事件处理子模块

use iced_winit::{conversion, winit};

use crate::host::Host;
use crate::message;

impl Host {
    pub(super) fn handle_keyboard_input_event(
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

    pub(super) fn handle_modifiers_changed_event(
        &mut self,
        new_modifiers: &winit::event::Modifiers,
    ) {
        let ctrl = super::super::is_ctrl_or_cmd_pressed(new_modifiers.state());
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
    pub(super) fn handle_touch_for_pinch(&mut self, touch: &winit::event::Touch) -> bool {
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
            if let Some(prev) = self.prev_pinch_distance
                && prev > 0.0
            {
                let delta = (dist / prev) - 1.0;
                // 夹到 ±0.15 避免首帧或快速张合跳变（类似 yinhe 0.02 阈值）
                let clamped = delta.clamp(-0.15, 0.15);
                if clamped.abs() > 0.01 {
                    self.handle_pinch_gesture(clamped as f64, winit::event::TouchPhase::Moved);
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
    pub(super) fn handle_pinch_gesture(&mut self, delta: f64, _phase: winit::event::TouchPhase) {
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
}
