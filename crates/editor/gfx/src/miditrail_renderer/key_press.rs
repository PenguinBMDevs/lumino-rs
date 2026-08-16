//! Miditrail 琴键按下效果
//!
//! 根据当前 tick 的激活音符更新每个键的按下系数，实现按下/回弹的平滑过渡。

use super::{ActiveKeys, MiditrailRenderer};

impl MiditrailRenderer {
    /// 更新 128 个键的按下系数。
    ///
    /// 按下速度比回弹速度稍快，使视觉效果更有力。
    /// `active_keys` 由 `super::instances::compute_active_keys` 预先计算；
    /// `fps` 为用户设置的目标帧率，决定每帧时间步长。
    pub(super) fn update_key_press_factors(&mut self, active_keys: &ActiveKeys, fps: f32) {
        let dt = 1.0 / fps.max(1.0);
        for (press, is_active) in self
            .key_press_factors
            .iter_mut()
            .zip(active_keys.pressed.iter())
        {
            let target = if *is_active { 1.0 } else { 0.0 };
            let speed = if target > *press {
                super::KEY_PRESS_SPEED_DOWN
            } else {
                super::KEY_PRESS_SPEED_UP
            };
            let new = *press + (target - *press) * speed * dt;
            *press = new.clamp(0.0, 1.0);
        }
    }
}
