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

    /// 更新弹簧物理模拟，返回是否仍在动画中
    pub fn update(&mut self) -> bool {
        if !self.active {
            return false;
        }

        let now = Instant::now();
        let dt = self
            .last_update
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(crate::constants::timing::DEFAULT_FRAME_TIME_SECS);
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
