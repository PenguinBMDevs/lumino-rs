//! Miditrail 琴键按下效果
//!
//! 根据当前 tick 的激活音符更新每个键的按下系数，实现按下/回弹的平滑过渡。

use super::{MiditrailNoteGpu, MiditrailRenderer, MiditrailUniformGpu};

impl MiditrailRenderer {
    /// 更新 128 个键的按下系数。
    ///
    /// 按下速度比回弹速度稍快，使视觉效果更有力。
    pub(super) fn update_key_press_factors(
        &mut self,
        uniform: &MiditrailUniformGpu,
        notes: &[MiditrailNoteGpu],
    ) {
        let tick = uniform.tick;
        let mut active = [false; 128];
        for note in notes {
            if note.is_active_at(tick) {
                let key = note.key as usize;
                if key < 128 {
                    active[key] = true;
                }
            }
        }

        // 假设视频帧率为 60fps，用固定时间步长平滑按下/回弹
        let dt = 1.0 / 60.0;
        for (press, is_active) in self.key_press_factors.iter_mut().zip(active.iter()) {
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
