//! AboutEggState 动画实现（从 about_egg.rs 拆出）

use super::*;

impl AboutEggState {
    /// 创建彩蛋状态机。
    ///
    /// 若进程内 logo 已消失，则直接以 [`EggPhase::Vanished`] 起始——这样「重开设置
    /// 面板不再出现」只需读一次进程级标志，视图层无需再关心它。
    #[must_use]
    pub fn new() -> Self {
        Self {
            clicks: 0,
            last_click: None,
            click_point: None,
            phase: if logo_has_vanished() {
                EggPhase::Vanished
            } else {
                EggPhase::Idle
            },
            phase_elapsed: 0.0,
            sequence_elapsed: 0.0,
            origin: (0.0, 0.0),
            target: (0.0, 0.0),
            position: (0.0, 0.0),
            gravity: 0.0,
            fall_velocity: 0.0,
            spin_angle: 0.0,
            opacity: 1.0,
            viewport: (0.0, 0.0),
            last_update: None,
            seed: Self::initial_seed(),
            impact_signal: false,
        }
    }

    /// 由系统时间派生非零随机种子
    fn initial_seed() -> u64 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        nanos | 1
    }

    /// 注入视口尺寸（窗口逻辑像素，由 Host 每帧调用）
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = (width.max(1.0), height.max(1.0));
    }

    /// 记录本次点击的指针位置（窗口逻辑像素）。
    ///
    /// 由 Host 在路由点击消息**之前**调用；`None` 表示无指针信息（正常鼠标点击不会
    /// 出现），此时动画起点回退到视口中心。
    pub fn set_click_point(&mut self, point: Option<(f32, f32)>) {
        self.click_point = point;
    }

    /// 当前连点计数（测试与日志用）
    #[must_use]
    pub fn clicks(&self) -> u32 {
        self.clicks
    }

    /// 当前相位（测试用）
    #[must_use]
    pub fn phase(&self) -> EggPhase {
        self.phase
    }

    /// 是否处于需要逐帧驱动的状态（晃动 / 彩蛋序列）
    #[must_use]
    pub fn is_animating(&self) -> bool {
        matches!(
            self.phase,
            EggPhase::Shake
                | EggPhase::Spin
                | EggPhase::Detach
                | EggPhase::Fall
                | EggPhase::Impact
                | EggPhase::FadeOut
        )
    }

    /// 是否处于「脱离原位后」的飞行阶段（决定是否需要悬浮层渲染 logo）
    #[must_use]
    pub fn is_airborne(&self) -> bool {
        matches!(
            self.phase,
            EggPhase::Spin
                | EggPhase::Detach
                | EggPhase::Fall
                | EggPhase::Impact
                | EggPhase::FadeOut
        )
    }

    /// logo 是否应在关于页内（原位）渲染
    #[must_use]
    pub fn is_visible_in_page(&self) -> bool {
        matches!(self.phase, EggPhase::Idle | EggPhase::Shake)
    }

    /// 取走「落地」一次性信号（消费式：同一帧只返回一次 `true`）。
    ///
    /// 调用方据此播放落地音效：进入 [`EggPhase::Impact`] 的那一帧置位，
    /// 取走后不再重复触发（相位重入、硬超时收尾都不会重复出声）。
    pub fn take_impact_signal(&mut self) -> bool {
        std::mem::take(&mut self.impact_signal)
    }

    /// 原点晃动位移（逻辑像素，正 = 右移）；非晃动相位恒为 0。
    ///
    /// 用「正弦 × 线性衰减包络」实现「晃动一次后自然收住」，起止位移均为 0，
    /// 因此不会在相位切换处留下跳变。
    #[must_use]
    pub fn shake_offset(&self) -> f32 {
        if self.phase != EggPhase::Shake {
            return 0.0;
        }
        let u = (self.phase_elapsed / SHAKE_DURATION).clamp(0.0, 1.0);
        ABOUT_LOGO_SHAKE_AMPLITUDE * (std::f32::consts::TAU * SHAKE_CYCLES * u).sin() * (1.0 - u)
    }

    /// 当前旋转角（弧度）
    #[must_use]
    pub fn spin_angle(&self) -> f32 {
        self.spin_angle
    }

    /// 旋转期的尺寸补偿系数（乘在 Svg 的固定边长上）。
    ///
    /// `iced_widget::svg::Svg` 以「旋转后包围盒」参与 `ContentFit::Contain` 的缩放
    /// （见 iced 0.14 `svg.rs` 的 `rotation.apply(image_size)`），若不做补偿，logo 在
    /// 旋转过程中会被等比缩小（45° 时约缩到 77%），观感是「转动时周期性一缩一放」。
    /// 这里按 AABB 长边反解，使**绘制尺寸恒定**。
    #[must_use]
    pub fn spin_fit_scale(&self) -> f32 {
        let (sin, cos) = (self.spin_angle.sin().abs(), self.spin_angle.cos().abs());
        let aabb_a = LOGO_ASPECT_W * cos + LOGO_ASPECT_H * sin;
        let aabb_b = LOGO_ASPECT_W * sin + LOGO_ASPECT_H * cos;
        (aabb_a.max(aabb_b) / LOGO_ASPECT_H).max(1.0)
    }

    /// 当前不透明度
    #[must_use]
    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    /// 当前方盒左上角位置（窗口逻辑像素）
    #[must_use]
    pub fn position(&self) -> (f32, f32) {
        self.position
    }

    /// 处理一次 logo 点击。
    ///
    /// 返回 `true` 表示本次点击被消费（触发晃动或触发彩蛋序列）；返回 `false` 表示
    /// 序列已在进行中或 logo 已消失，点击被忽略。
    pub fn on_logo_click(&mut self, now: Instant) -> bool {
        if self.phase == EggPhase::Vanished || self.is_airborne() {
            return false;
        }

        // 超时清零：距上次点击超过阈值则重新累计（惰性判定，无需定时器）
        if let Some(last) = self.last_click
            && now.duration_since(last) > ABOUT_EGG_CLICK_TIMEOUT
        {
            self.clicks = 0;
        }
        self.last_click = Some(now);
        self.clicks = self.clicks.saturating_add(1);

        if self.clicks >= ABOUT_EGG_CLICK_THRESHOLD {
            self.start_sequence(now);
            return true;
        }

        // 未达阈值：原地晃动一次（重复点击则重新起振）
        self.phase = EggPhase::Shake;
        self.phase_elapsed = 0.0;
        self.last_update = Some(now);
        true
    }

    /// 推进动画；返回是否仍在动画中。
    ///
    /// `dt` 上限钳 50ms：卡顿/断点后不会瞬移，也不会因为一次超大 dt 直接跳过整段序列。
    pub fn update(&mut self, now: Instant) -> bool {
        if !self.is_animating() {
            self.last_update = Some(now);
            return false;
        }

        let dt = match self.last_update {
            Some(prev) => now.saturating_duration_since(prev).as_secs_f32().min(0.05),
            None => 0.0,
        };
        self.last_update = Some(now);
        self.phase_elapsed += dt;

        // 硬超时只统计**序列**时长（晃动不计入），否则反复晃动会误触兜底收尾
        if self.is_airborne() {
            self.sequence_elapsed += dt;
            if self.sequence_elapsed > ABOUT_EGG_HARD_TIMEOUT.as_secs_f32() {
                self.finish();
                return false;
            }
        }

        match self.phase {
            EggPhase::Shake => {
                if self.phase_elapsed >= SHAKE_DURATION {
                    self.phase = EggPhase::Idle;
                    self.phase_elapsed = 0.0;
                    return false;
                }
            }
            EggPhase::Spin => {
                self.advance_spin(dt);
                if self.phase_elapsed >= SPIN_DURATION {
                    self.enter(EggPhase::Detach);
                }
            }
            EggPhase::Detach => {
                self.advance_spin(dt);
                let u = (self.phase_elapsed / DETACH_DURATION).clamp(0.0, 1.0);
                let ease = 1.0 - (1.0 - u).powi(3);
                self.position = (
                    lerp(self.origin.0, self.target.0, ease),
                    lerp(self.origin.1, self.target.1, ease),
                );
                if u >= 1.0 {
                    self.begin_fall();
                }
            }
            EggPhase::Fall => {
                self.advance_spin(dt);
                self.fall_velocity += self.gravity * dt;
                self.position.1 += self.fall_velocity * dt;
                let floor = self.floor_y();
                if self.position.1 >= floor {
                    self.position.1 = floor;
                    self.enter(EggPhase::Impact);
                    // 落地一次性信号：调用方在同一帧取走并播放音效
                    self.impact_signal = true;
                }
            }
            EggPhase::Impact => {
                // 音效不在这里播放：落地那一帧已置位 `impact_signal`，由 `lumino-ui`
                // 取走后交给 `lumino_midi_io::ui_sfx` 播放（ui-core 不引入音频依赖）。
                // 本相位只保留一个短暂停顿，作为「砸到底」的视觉/听觉落点。
                if self.phase_elapsed >= IMPACT_DURATION {
                    self.enter(EggPhase::FadeOut);
                }
            }
            EggPhase::FadeOut => {
                let u = (self.phase_elapsed / FADE_DURATION).clamp(0.0, 1.0);
                self.opacity = 1.0 - u;
                if u >= 1.0 {
                    self.finish();
                    return false;
                }
            }
            EggPhase::Idle | EggPhase::Vanished => return false,
        }

        true
    }

    /// 触发彩蛋序列：采样随机落点（中轴 Y 以上）并进入快速转动
    fn start_sequence(&mut self, now: Instant) {
        self.clicks = 0;
        self.phase = EggPhase::Spin;
        self.phase_elapsed = 0.0;
        self.sequence_elapsed = 0.0;
        self.last_update = Some(now);
        self.spin_angle = 0.0;
        self.opacity = 1.0;
        self.fall_velocity = 0.0;

        let (w, h) = self.viewport;
        let half = ABOUT_LOGO_BOX * 0.5;

        // 起点 = 点击点（点击必落在 logo 内，故该点即 logo 视觉中心）
        let (cx, cy) = self.click_point.unwrap_or((w * 0.5, h * 0.35));
        self.origin = (
            clamp_inside(cx - half, 0.0, (w - ABOUT_LOGO_BOX).max(0.0)),
            clamp_inside(cy - half, 0.0, (h - ABOUT_LOGO_BOX).max(0.0)),
        );
        self.position = self.origin;

        // 落点：X 任意，Y 严格落在窗口**中轴以上**
        let max_x = (w - ABOUT_LOGO_BOX - FLIGHT_MARGIN).max(FLIGHT_MARGIN);
        let max_y = (h * 0.5 - ABOUT_LOGO_BOX - FLIGHT_MARGIN).max(FLIGHT_MARGIN);
        let rnd_x = self.next_unit();
        let rnd_y = self.next_unit();
        self.target = (
            lerp(FLIGHT_MARGIN, max_x, rnd_x),
            lerp(FLIGHT_MARGIN, max_y, rnd_y),
        );
    }

    /// 进入重力下落：按落点距离反解加速度，保证下落时长稳定（"从简匀加速"）
    fn begin_fall(&mut self) {
        self.position = self.target;
        self.fall_velocity = 0.0;
        let distance = (self.floor_y() - self.target.1).max(0.0);
        self.gravity = 2.0 * distance / (FALL_DURATION * FALL_DURATION);
        self.enter(EggPhase::Fall);
    }

    /// 窗口底部（方盒左上角的极限 Y）
    fn floor_y(&self) -> f32 {
        (self.viewport.1 - ABOUT_LOGO_BOX - FLIGHT_MARGIN).max(0.0)
    }

    /// 推进旋转角
    fn advance_spin(&mut self, dt: f32) {
        let rate = std::f32::consts::TAU * SPIN_TURNS / SPIN_DURATION;
        self.spin_angle += rate * dt;
    }

    /// 切换相位并重置相位计时
    fn enter(&mut self, phase: EggPhase) {
        self.phase = phase;
        self.phase_elapsed = 0.0;
    }

    /// 收尾：标记进程级消失并复位计数（不持久化，重启即恢复）
    fn finish(&mut self) {
        self.phase = EggPhase::Vanished;
        self.phase_elapsed = 0.0;
        self.clicks = 0;
        self.last_click = None;
        self.spin_angle = 0.0;
        self.opacity = 0.0;
        // 硬超时兜底收尾时可能尚有未取走的落地信号：一并清掉，避免延迟出声
        self.impact_signal = false;
        mark_logo_vanished();
    }

    /// xorshift64 → [0, 1) 均匀分布（避免为一次随机采样引入 `rand` 依赖）
    fn next_unit(&mut self) -> f32 {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;
        ((x >> 11) as f64 / (1u64 << 53) as f64) as f32
    }
}

/// 线性插值
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 把 `v` 钳制到 `[lo, hi]`；`hi < lo` 时退化为 `lo`（窗口过小时的兜底）
fn clamp_inside(v: f32, lo: f32, hi: f32) -> f32 {
    if hi <= lo { lo } else { v.clamp(lo, hi) }
}
