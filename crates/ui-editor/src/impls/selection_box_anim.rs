//! 框选框弹簧物理动画
//!
//! 包含：`update_selection_box_animation` 方法。
//!
//! 拆分原因：`editor_impl.rs` 接近 400 行限制，按职责拆分。

use crate::Editor;
use iced_core::Point;

impl Editor {
    /// 更新框选框的弹簧物理动画
    ///
    /// 使用弹簧物理模拟让选择框边界产生 Q 弹的弹性效果。
    /// 以 snap_precision 为精度单位"跳跃"，在跳跃之间使用弹簧动画过渡：
    /// - 鼠标移动时，先计算吸附到网格的目标位置
    /// - 只有当吸附位置发生变化时，才更新弹簧目标
    /// - 弹簧以弹性方式从上一个吸附位置过渡到新的吸附位置
    /// - 弹簧收敛后标记 converged，供 frame.rs 停止 AnimationTick 轮询
    ///
    /// `mouse_pos`:
    /// - `Some(pos)`: 鼠标移动中，重新计算吸附目标
    /// - `None`: 持续推进弹簧物理向现有目标收敛（用于 AnimationTick）
    pub fn update_selection_box_animation(&self, mouse_pos: Option<Point>) {
        use crate::EditState;
        use crate::SelectionBoxAnimState;
        use lumino_core::storage::config::SelectionBoxMode;

        // 直接跟随模式：不需要弹簧动画，直接返回
        if self.editor_state.view.selection_box_mode == SelectionBoxMode::Direct {
            // 清除任何残留的动画状态
            self.selection_box_anim.set(None);
            return;
        }

        let interaction = &self.editor_state.interaction;

        match interaction.edit_state {
            EditState::Selecting {
                start_tick,
                start_key,
                current_tick,
                current_key,
                ..
            } => {
                // 计算起点的屏幕坐标（固定锚点）
                let start_x = self.tick_to_x(start_tick);
                let start_y = self.key_to_y(start_key);
                let start_pos = Point::new(start_x, start_y);

                // 计算吸附后的目标位置
                let snapped_tick = if let Some(pos) = mouse_pos {
                    let tick = self.x_to_tick(pos.x);
                    self.snap_tick(tick)
                } else {
                    current_tick
                };
                let snapped_key = if let Some(pos) = mouse_pos {
                    self.y_to_key(pos.y)
                } else {
                    current_key
                };

                // 获取或初始化动画状态
                let anim = self.selection_box_anim.get();

                let (display_current, mut velocity, last_snapped_tick, last_snapped_key) =
                    if let Some(state) = anim {
                        (
                            state.current_pos,
                            state.velocity,
                            state.snapped_tick,
                            state.snapped_key,
                        )
                    } else {
                        // 初始状态：显示位置等于第一个吸附位置
                        let init_x = self.tick_to_x(snapped_tick);
                        let init_y = self.key_to_y(snapped_key);
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
                    let target_x = self.tick_to_x(snapped_tick);
                    let target_y = self.key_to_y(snapped_key);
                    Point::new(target_x, target_y)
                } else {
                    let target_x = self.tick_to_x(last_snapped_tick);
                    let target_y = self.key_to_y(last_snapped_key);
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

                // 弹簧收敛判断：位置和速度都足够接近目标时标记收敛
                let dx = current.x - spring_target.x;
                let dy = current.y - spring_target.y;
                let dist_sq = dx * dx + dy * dy;
                let speed_sq = velocity.x * velocity.x + velocity.y * velocity.y;
                const POS_THRESHOLD_SQ: f32 = 0.25; // 0.5 像素的平方
                const VEL_THRESHOLD_SQ: f32 = 0.01; // 0.1 像素/帧的平方

                let converged = dist_sq < POS_THRESHOLD_SQ && speed_sq < VEL_THRESHOLD_SQ;

                self.selection_box_anim.set(Some(SelectionBoxAnimState {
                    start_pos,
                    current_pos: current,
                    velocity,
                    snapped_tick,
                    snapped_key,
                    converged,
                }));
            }
            _ => {
                // 非选择状态，清除动画状态
                self.selection_box_anim.set(None);
            }
        }
    }
}
