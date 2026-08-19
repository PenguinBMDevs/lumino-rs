//! 钢琴卷帘网格绘制程序

use super::state::GridInteractionState;
use crate::Editor;
use iced_core::Point;
use iced_widget::canvas::{self};
use lumino_ui_core::Message;
use lumino_ui_core::constants::editor as editor_constants;
use lumino_ui_core::constants::editor::{SCROLL_LINES_SCALE, SCROLL_MAX_DELTA};

/// 钢琴卷帘网格绘制程序，负责处理网格区域的鼠标交互与绘制。
pub struct PianoRollGrid<'a> {
    /// 编辑器实例的引用
    pub editor: &'a Editor,
}

impl<'a> PianoRollGrid<'a> {
    /// 创建网格绘制程序。
    ///
    /// # 参数
    /// * `editor` — 编辑器实例引用
    ///
    /// # 返回
    /// 绑定到指定编辑器的 `PianoRollGrid`。
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }

    pub(super) fn detect_double_click(
        &self,
        state: &mut GridInteractionState,
        local_pos: Point,
    ) -> bool {
        use editor_constants::*;

        let now = std::time::Instant::now();
        let is_double_click = state.last_click_pos.is_some_and(|last_pos| {
            let time_delta = now.duration_since(state.last_click_time).as_millis();
            let pos_delta =
                ((local_pos.x - last_pos.x).powi(2) + (local_pos.y - last_pos.y).powi(2)).sqrt();
            time_delta < DOUBLE_CLICK_TIME_MS && pos_delta < DOUBLE_CLICK_DISTANCE_PX
        });

        if !is_double_click {
            state.last_click_time = now;
            state.last_click_pos = Some(local_pos);
        }

        is_double_click
    }

    pub(super) fn handle_left_press(
        &self,
        state: &mut GridInteractionState,
        local_pos: Point,
    ) -> Option<canvas::Action<Message>> {
        use lumino_ui_core::message::EditorAction;

        let v = &self.editor.editor_state.view;
        if local_pos.y < v.ruler_height && local_pos.x >= v.keyboard_width {
            // 先检测是否点击到循环区域
            if let Some(loop_range) = self.editor.loop_range.as_ref()
                && loop_range.enabled()
            {
                let loop_start_x =
                    loop_range.start_tick() * v.zoom_x - v.scroll_x + v.keyboard_width;
                let loop_end_x = loop_range.end_tick() * v.zoom_x - v.scroll_x + v.keyboard_width;
                if local_pos.x >= loop_start_x && local_pos.x <= loop_end_x {
                    state.is_loop_dragging = true;
                    return Some(canvas::Action::publish(Message::LoopRange(
                        lumino_ui_core::message::LoopRangeAction::RulerPressed {
                            x: local_pos.x,
                            y: local_pos.y,
                        },
                    )));
                }
            }

            // 固定指示线模式下：检测是否点击到指示线本身（支持拖拽）
            let asc = self.editor.editor_state.auto_scroll;
            if asc.mode == lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft {
                let indicator_screen_x = self
                    .editor
                    .get_playback_indicator_screen_x()
                    .unwrap_or(v.keyboard_width);
                let hit_margin = 8.0; // 点击容差
                if (local_pos.x - indicator_screen_x).abs() <= hit_margin {
                    state.is_dragging_indicator = true;
                    return Some(canvas::Action::publish(Message::EditorAction(
                        EditorAction::IndicatorDragStart { x: local_pos.x },
                    )));
                }
            }

            let tick = self.editor.x_to_tick(local_pos.x);
            let snapped_tick = self.editor.snap_tick(tick).max(0.0);
            return Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::Scrubbed { tick: snapped_tick },
            )));
        }

        if self.detect_double_click(state, local_pos) {
            Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::DoubleClicked(lumino_ui_core::message::Point2::new(
                    local_pos.x,
                    local_pos.y,
                )),
            )))
        } else {
            Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::Pressed {
                    pos: lumino_ui_core::message::Point2::new(local_pos.x, local_pos.y),
                    shift: state.shift_pressed,
                },
            )))
        }
    }

    pub(super) fn handle_wheel_scroll(
        &self,
        delta: &iced_core::mouse::ScrollDelta,
        shift_pressed: bool,
    ) -> Option<canvas::Action<Message>> {
        use lumino_ui_core::message::EditorAction;

        let (mut delta_x, mut delta_y) = Self::wheel_delta(delta);

        // Shift+滚轮：将垂直滚动转换为水平滚动
        // 部分平台已自动转换（delta_x 非零），未转换的平台需要手动处理
        // 注意：handle_scrolled 中 X/Y 两轴均为 scroll - delta（取反累加），
        // 因此垂直增量直接平移到水平轴即可保持语义一致：
        // 向下滚（delta_y < 0）→ delta_x < 0 → scroll_x 增大（视图右移）。
        if shift_pressed && delta_x.abs() < f32::EPSILON {
            delta_x = delta_y;
            delta_y = 0.0;
        }

        let delta_x = delta_x.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
        let delta_y = delta_y.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);

        Some(canvas::Action::publish(Message::EditorAction(
            EditorAction::Scrolled { delta_x, delta_y },
        )))
    }

    /// 标尺区域（顶部小节号栏）滚轮处理：
    /// - Ctrl+滚轮：X 轴缩放，以鼠标位置为缩放锚点
    /// - 普通滚轮：水平平移（向上滚向右、向下滚向左移动视图）
    pub(super) fn handle_ruler_wheel_scroll(
        &self,
        delta: &iced_core::mouse::ScrollDelta,
        control_pressed: bool,
        local_pos: Point,
    ) -> Option<canvas::Action<Message>> {
        use lumino_ui_core::message::EditorAction;

        if control_pressed {
            let factor = crate::zoom::zoom_factor_from_delta(delta)?;
            let view = &self.editor.editor_state.view;
            let canvas = &self.editor.editor_state.canvas;
            let viewport_w = (canvas.size_x - view.keyboard_width).max(0.0);
            return Some(canvas::Action::publish(Message::ZoomXChanged {
                zoom: view.zoom_x * factor,
                fixed_ratio: crate::zoom::fixed_ratio_from_viewport(
                    local_pos.x,
                    view.keyboard_width,
                    viewport_w,
                ),
            }));
        }

        // 普通滚轮：垂直增量映射为水平移动。向上滚（delta_y > 0）向右移动视图
        // （scroll_x 增大），向下滚（delta_y < 0）向左移动视图。
        // 注意取反：handle_scrolled 中两轴均为 scroll - delta（取反累加），
        // 发送 -delta_y 才能保持"向上滚 → 视图右移"的既有语义。
        let (_, delta_y) = Self::wheel_delta(delta);
        let delta_y = delta_y.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
        if delta_y.abs() < f32::EPSILON {
            return None;
        }
        Some(canvas::Action::publish(Message::EditorAction(
            EditorAction::Scrolled {
                delta_x: -delta_y,
                delta_y: 0.0,
            },
        )))
    }

    /// 键盘区域（左侧琴键栏）滚轮处理：
    /// Ctrl+滚轮：Y 轴缩放，以鼠标位置为缩放锚点；无 Ctrl 时无操作（保持原行为，不干扰标记）。
    pub(super) fn handle_keyboard_wheel_scroll(
        &self,
        delta: &iced_core::mouse::ScrollDelta,
        control_pressed: bool,
        local_pos: Point,
    ) -> Option<canvas::Action<Message>> {
        if !control_pressed {
            return None;
        }
        let factor = crate::zoom::zoom_factor_from_delta(delta)?;
        let view = &self.editor.editor_state.view;
        let canvas = &self.editor.editor_state.canvas;
        let viewport_h = (canvas.size_y - view.ruler_height).max(0.0);
        Some(canvas::Action::publish(Message::ZoomYChanged {
            zoom: view.zoom_y * factor,
            fixed_ratio: crate::zoom::fixed_ratio_from_viewport(
                local_pos.y,
                view.ruler_height,
                viewport_h,
            ),
        }))
    }

    /// 解析滚轮增量（Lines 乘以滚动刻度系数，Pixels 原样）→ (delta_x, delta_y)
    fn wheel_delta(delta: &iced_core::mouse::ScrollDelta) -> (f32, f32) {
        match delta {
            iced_core::mouse::ScrollDelta::Lines { x, y } => {
                (*x * SCROLL_LINES_SCALE, *y * SCROLL_LINES_SCALE)
            }
            iced_core::mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
        }
    }

    /// 更新框选框的弹簧物理动画
    ///
    /// 委托给 Editor::update_selection_box_animation 执行。
    pub fn update_selection_box_animation(&self, mouse_pos: Option<Point>) {
        self.editor.update_selection_box_animation(mouse_pos);
    }
}

#[cfg(test)]
mod tests;
