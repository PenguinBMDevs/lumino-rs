//! 钢琴卷帘网格绘制程序

use super::state::GridInteractionState;
use crate::Message;
use crate::constants::editor as editor_constants;
use crate::editor::Editor;
use iced_core::Point;
use iced_widget::canvas::{self};

pub struct PianoRollGrid<'a> {
    pub editor: &'a Editor,
}

impl<'a> PianoRollGrid<'a> {
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
        use crate::message::EditorAction;

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
                        crate::message::LoopRangeAction::RulerPressed {
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
                EditorAction::DoubleClicked(local_pos),
            )))
        } else {
            Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::Pressed {
                    pos: local_pos,
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
        use crate::message::EditorAction;
        use editor_constants::*;

        let (mut delta_x, mut delta_y) = match delta {
            iced_core::mouse::ScrollDelta::Lines { x, y } => {
                (*x * SCROLL_LINES_SCALE, *y * SCROLL_LINES_SCALE)
            }
            iced_core::mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
        };

        // Shift+滚轮：将垂直滚动转换为水平滚动
        // 部分平台已自动转换（delta_x 非零），未转换的平台需要手动处理
        // 注意取反：handle_scrolled 中 X 轴是 scroll_x + delta_x（直接加），
        // Y 轴是 scroll_y - delta_y（取反减），所以垂直→水平映射时必须取反符号。
        if shift_pressed && delta_x.abs() < f32::EPSILON {
            delta_x = -delta_y;
            delta_y = 0.0;
        }

        let delta_x = delta_x.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
        let delta_y = delta_y.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);

        Some(canvas::Action::publish(Message::EditorAction(
            EditorAction::Scrolled { delta_x, delta_y },
        )))
    }

    /// 更新框选框的弹簧物理动画
    ///
    /// 委托给 Editor::update_selection_box_animation 执行。
    pub(super) fn update_selection_box_animation(&self, mouse_pos: Option<Point>) {
        self.editor.update_selection_box_animation(mouse_pos);
    }
}
