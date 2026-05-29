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

    /// 更新框选框的弹簧物理动画
    ///
    /// 使用弹簧物理模拟让选择框边界产生 Q 弹的弹性效果。
    /// 以 snap_precision 为精度单位"跳跃"，在跳跃之间使用弹簧动画过渡：
    /// - 鼠标移动时，先计算吸附到网格的目标位置
    /// - 只有当吸附位置发生变化时，才更新弹簧目标
    /// - 弹簧以弹性方式从上一个吸附位置过渡到新的吸附位置
    pub(super) fn update_selection_box_animation(&self, mouse_pos: Option<Point>) {
        use crate::editor::{EditState, SelectionBoxAnimState};

        let interaction = &self.editor.editor_state.interaction;

        match interaction.edit_state {
            EditState::Selecting {
                start_tick,
                start_key,
                current_tick,
                current_key,
                ..
            } => {
                // 计算起点的屏幕坐标（固定锚点）
                let start_x = self.editor.tick_to_x(start_tick);
                let start_y = self.editor.key_to_y(start_key);
                let start_pos = Point::new(start_x, start_y);

                // 计算吸附后的目标位置（使用 snap 精度）
                let snapped_tick = if let Some(pos) = mouse_pos {
                    let tick = self.editor.x_to_tick(pos.x);
                    self.editor.snap_tick(tick)
                } else {
                    current_tick
                };
                let snapped_key = if let Some(pos) = mouse_pos {
                    self.editor.y_to_key(pos.y)
                } else {
                    current_key
                };

                // 获取或初始化动画状态
                let mut anim = self.editor.selection_box_anim.borrow_mut();

                let (display_current, mut velocity, last_snapped_tick, last_snapped_key) =
                    if let Some(state) = *anim {
                        (
                            state.current_pos,
                            state.velocity,
                            state.snapped_tick,
                            state.snapped_key,
                        )
                    } else {
                        // 初始状态：显示位置等于第一个吸附位置
                        let init_x = self.editor.tick_to_x(snapped_tick);
                        let init_y = self.editor.key_to_y(snapped_key);
                        (
                            Point::new(init_x, init_y),
                            Point::new(0.0, 0.0),
                            snapped_tick,
                            snapped_key,
                        )
                    };

                // 判断吸附位置是否发生变化
                let snapped_changed =
                    snapped_tick != last_snapped_tick || snapped_key != last_snapped_key;

                // 计算弹簧目标位置：吸附位置变化时更新目标，否则保持上一次的目标
                let spring_target = if snapped_changed {
                    let target_x = self.editor.tick_to_x(snapped_tick);
                    let target_y = self.editor.key_to_y(snapped_key);
                    Point::new(target_x, target_y)
                } else {
                    let target_x = self.editor.tick_to_x(last_snapped_tick);
                    let target_y = self.editor.key_to_y(last_snapped_key);
                    Point::new(target_x, target_y)
                };

                // 弹簧物理参数（Q弹效果）
                const STIFFNESS: f32 = 400.0; // 弹簧刚度（越大回弹越快）
                const DAMPING: f32 = 15.0; // 阻尼系数（越小越弹）
                const MASS: f32 = 1.0; // 质量
                const DT: f32 = 1.0 / 60.0; // 固定时间步长（假设60fps）
                const SUB_STEPS: i32 = 4; // 每帧子步数，提高稳定性

                let mut current = display_current;

                // 半隐式欧拉积分，多子步提高稳定性
                for _ in 0..SUB_STEPS {
                    let dt = DT / SUB_STEPS as f32;

                    // 计算弹簧力（胡克定律）
                    let displacement_x = spring_target.x - current.x;
                    let displacement_y = spring_target.y - current.y;
                    let spring_force_x = STIFFNESS * displacement_x;
                    let spring_force_y = STIFFNESS * displacement_y;

                    // 计算阻尼力
                    let damping_force_x = DAMPING * velocity.x;
                    let damping_force_y = DAMPING * velocity.y;

                    // 计算加速度（F = ma => a = F/m）
                    let accel_x = (spring_force_x - damping_force_x) / MASS;
                    let accel_y = (spring_force_y - damping_force_y) / MASS;

                    // 更新速度和位置
                    velocity.x += accel_x * dt;
                    velocity.y += accel_y * dt;
                    current.x += velocity.x * dt;
                    current.y += velocity.y * dt;
                }

                *anim = Some(SelectionBoxAnimState {
                    start_pos,
                    current_pos: current,
                    velocity,
                    snapped_tick,
                    snapped_key,
                });
            }
            _ => {
                // 非选择状态，清除动画状态
                *self.editor.selection_box_anim.borrow_mut() = None;
            }
        }
    }
}
