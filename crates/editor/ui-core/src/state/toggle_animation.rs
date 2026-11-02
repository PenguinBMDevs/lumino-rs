//! 模式切换按钮的弹簧物理动画状态

use std::time::Instant;

/// 模式切换按钮的弹簧物理动画状态
#[derive(Debug, Clone)]
pub struct ToggleAnimationState {
    /// 动画进度 (0.0 = Editor, 1.0 = Waterfall)
    pub position: f32,
    /// 速度（用于弹簧物理模拟）
    pub velocity: f32,
    /// 目标位置
    pub target: f32,
    /// 是否正在动画中
    pub active: bool,
    /// 上次更新时间（用于计算 dt）
    pub last_update: Option<Instant>,
}

impl Default for ToggleAnimationState {
    fn default() -> Self {
        Self {
            position: 0.0,
            velocity: 0.0,
            target: 0.0,
            active: false,
            last_update: None,
        }
    }
}

impl ToggleAnimationState {
    const STIFFNESS: f64 = 200.0;
    const DAMPING: f64 = 15.0;
    const VELOCITY_THRESHOLD: f64 = 0.001;
    const POSITION_THRESHOLD: f64 = 0.001;

    /// 创建一个默认的开关动画状态
    pub fn new() -> Self {
        Self::default()
    }

    /// 启动动画到目标位置
    pub fn animate_to(&mut self, target: f32) {
        self.target = target;
        if !self.active {
            self.active = true;
            self.last_update = Some(Instant::now());
        }
    }

    /// 是否正在**朝向瀑布流**（含「已到目标但本帧尚未同步 `AppMode`」的窗口）。
    ///
    /// 只判 `target >= 0.5`：`handle_animation_tick` 用 0.5 作为 `AppMode` 归属阈值，
    /// 因此只要目标已指向瀑布流，**无论当前 `position` 是多少、动画是否已收敛**，
    /// 后续 tick 都可能把 `AppMode` 翻成 Waterfall。
    ///
    /// 这是给「切换闸/入口闸」用的过渡态判定：只看 `current_mode` 会漏掉
    /// 「点击进瀑布流（动画进行中，mode 仍是 Editor）→ 此刻切到 CPU 后端 →
    /// 动画收敛后 mode 变成 Waterfall」的竞态（CPU 后端不支持瀑布流）。
    #[must_use]
    pub fn is_heading_to_waterfall(&self) -> bool {
        self.target >= 0.5
    }

    /// 立即取消动画并把进度归零（编辑器态）。
    ///
    /// 用于「目标状态已不可达」的场景（如 CPU 洋葱皮后端不支持瀑布流）：
    /// 停在半途会让 `AppMode` 归属判定悬空，必须显式落到 Editor 并清速度。
    pub fn cancel_to_editor(&mut self) {
        self.target = 0.0;
        self.position = 0.0;
        self.velocity = 0.0;
        self.active = false;
        self.last_update = None;
    }

    /// 更新弹簧物理模拟，返回是否仍在动画中
    pub fn update(&mut self) -> bool {
        if !self.active {
            return false;
        }

        let now = Instant::now();
        let dt = self
            .last_update
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(1.0 / 60.0);
        self.last_update = Some(now);

        let dt = dt.min(0.05);

        let displacement = (self.position - self.target) as f64;
        let spring_force = -Self::STIFFNESS * displacement;
        let damping_force = -Self::DAMPING * self.velocity as f64;
        let acceleration = spring_force + damping_force;

        self.velocity += (acceleration * dt) as f32;
        self.position += self.velocity * dt as f32;

        let at_target = ((self.position - self.target).abs() as f64) < Self::POSITION_THRESHOLD
            && (self.velocity.abs() as f64) < Self::VELOCITY_THRESHOLD;

        if at_target {
            self.position = self.target;
            self.velocity = 0.0;
            self.active = false;
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ToggleAnimationState;

    /// 静止在编辑器态：不构成「正在进入瀑布流」。
    #[test]
    fn test_idle_editor_is_not_heading_to_waterfall() {
        assert!(!ToggleAnimationState::default().is_heading_to_waterfall());
    }

    /// 竞态的核心：点击进瀑布流的**同一帧**启动动画，`position` 仍为 0，
    /// 但目标已指向瀑布流 —— 此时必须判定为「正在进入瀑布流」。
    #[test]
    fn test_started_transition_is_heading_to_waterfall_before_position_moves() {
        let mut anim = ToggleAnimationState::default();
        anim.animate_to(1.0);
        assert_eq!(anim.position, 0.0, "弹簧动画刚启动时位置尚未移动");
        assert!(
            anim.is_heading_to_waterfall(),
            "目标已指向瀑布流即算过渡态（不能只看 position）"
        );
    }

    /// 已收敛到瀑布流（动画结束、AppMode 已同步）：仍判定为朝向瀑布流。
    #[test]
    fn test_converged_waterfall_still_heading() {
        let mut anim = ToggleAnimationState::default();
        anim.animate_to(1.0);
        anim.position = 1.0;
        anim.velocity = 0.0;
        anim.active = false;
        assert!(anim.is_heading_to_waterfall());
    }

    /// 退回编辑器（含动画进行中）：不再算「正在进入瀑布流」。
    #[test]
    fn test_returning_to_editor_is_not_heading_to_waterfall() {
        let mut anim = ToggleAnimationState::default();
        anim.animate_to(1.0);
        anim.position = 1.0;
        anim.animate_to(0.0);
        assert!(
            !anim.is_heading_to_waterfall(),
            "目标回到编辑器后，即便 position 还在瀑布流侧也不得判为进入瀑布流"
        );
    }

    /// 取消：目标/进度/速度全归零且不再活跃（`AppMode` 判定落到 Editor）。
    #[test]
    fn test_cancel_to_editor_resets_all_animation_fields() {
        let mut anim = ToggleAnimationState::default();
        anim.animate_to(1.0);
        anim.position = 0.7;
        anim.velocity = 3.0;

        anim.cancel_to_editor();

        assert_eq!(anim.target, 0.0);
        assert_eq!(anim.position, 0.0);
        assert_eq!(anim.velocity, 0.0);
        assert!(!anim.active);
        assert!(anim.last_update.is_none());
        assert!(!anim.is_heading_to_waterfall());
    }
}
