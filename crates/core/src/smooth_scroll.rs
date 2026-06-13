//! 平滑滚动动画模块

/// 平滑滚动动画状态
#[derive(Debug, Clone)]
pub struct SmoothScrollAnimation {
    /// 水平方向目标滚动位置
    pub target_x: f32,
    /// 垂直方向目标滚动位置
    pub target_y: f32,
    /// 是否正在动画中
    pub active: bool,
    /// 插值因子（0.0-1.0，越大动画越快）
    pub factor: f32,
    /// 停止阈值（距离小于此值时直接吸附到目标）
    pub threshold: f32,
}

impl Default for SmoothScrollAnimation {
    fn default() -> Self {
        Self {
            target_x: 0.0,
            target_y: 0.0,
            active: false,
            factor: 0.25,
            threshold: 0.5,
        }
    }
}

impl SmoothScrollAnimation {
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置目标位置并启动动画
    pub fn set_target(&mut self, target_x: f32, target_y: f32) {
        self.target_x = target_x;
        self.target_y = target_y;
        self.active = true;
    }

    /// 更新当前位置向目标位置逼近
    pub fn update(&self, current_x: f32, current_y: f32) -> (f32, f32, bool) {
        if !self.active {
            return (current_x, current_y, false);
        }
        let dx = self.target_x - current_x;
        let dy = self.target_y - current_y;
        if dx.abs() < self.threshold && dy.abs() < self.threshold {
            return (self.target_x, self.target_y, false);
        }
        let new_x = current_x + dx * self.factor;
        let new_y = current_y + dy * self.factor;
        (new_x, new_y, true)
    }

    /// 停止动画
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// 同步目标到当前位置
    pub fn sync(&mut self, current_x: f32, current_y: f32) {
        self.target_x = current_x;
        self.target_y = current_y;
        self.active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smooth_scroll_basic() {
        let mut anim = SmoothScrollAnimation::new();
        anim.set_target(100.0, 200.0);
        let (x, y, active) = anim.update(0.0, 0.0);
        assert!(active);
        assert!(x > 0.0 && x < 100.0);
        assert!(y > 0.0 && y < 200.0);
    }

    #[test]
    fn test_smooth_scroll_reaches_target() {
        let mut anim = SmoothScrollAnimation::new();
        anim.factor = 0.5;
        anim.threshold = 1.0;
        anim.set_target(10.0, 20.0);
        let mut x = 0.0;
        let mut y = 0.0;
        let mut active = true;
        let mut iterations = 0;
        while active && iterations < 100 {
            (x, y, active) = anim.update(x, y);
            if active {
                anim.set_target(10.0, 20.0);
            }
            iterations += 1;
        }
        assert!(!active);
        assert_eq!(x, 10.0);
        assert_eq!(y, 20.0);
    }
}
