//! 「关于」页 Lumino Logo 点击彩蛋状态机（UI-007）
//!
//! 行为约定（与需求卡逐条对应）：
//! - 单击 logo：原地晃动一次（{@link ABOUT_LOGO_SHAKE_AMPLITUDE} 像素振幅、衰减收尾）；
//! - 连点计数：累计满 [`ABOUT_EGG_CLICK_THRESHOLD`] 次触发彩蛋；两次点击间隔超过
//!   [`ABOUT_EGG_CLICK_TIMEOUT`] 则计数清零重新累计（**惰性判定**，不需要定时器）；
//! - 彩蛋序列：快速转动 → 脱离原位 → 飞到窗口内**中轴 Y 以上**的随机位置 →
//!   以匀加速（自然加速）砸向窗口底部 → 打入位停顿（音效挂载点）→ 渐隐消失；
//! - 消失状态：进程级 `static`，**写入配置持久化之外**，重启进程即恢复。
//!
//! 几何口径：全部使用**窗口逻辑像素**，坐标系原点 = 窗口左上角（与 iced 视口一致）。
//! 视口尺寸由 Host 每帧注入（[`AboutEggState::set_viewport`]），因此本模块**不依赖**
//! iced 布局、也不需要在视图层回读控件位置（iced 0.14 不提供该能力）。
//!
//! 时间推进全部由外部传入 `Instant`（[`AboutEggState::on_logo_click`] /
//! [`AboutEggState::update`]），因此单元测试可直接构造时间轴，无需 `sleep`。

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// 连点触发阈值（累计满该次数触发彩蛋序列）
pub const ABOUT_EGG_CLICK_THRESHOLD: u32 = 15;

/// 连点超时：距上一次点击超过该时长未继续点击，计数清零重新累计
pub const ABOUT_EGG_CLICK_TIMEOUT: Duration = Duration::from_secs(2);

/// 彩蛋序列硬超时（兜底）。
///
/// 序列正常耗时约 1.6~1.8s。硬超时用于保证「任何相位逻辑异常都一定会收尾」——
/// 否则动画状态可能长期为真，导致设置对话框持续逐帧重绘（仓库历史上出现过同类
/// 重绘自循环导致的 GPU 满载事故，见 `host/event/window/events.rs` 注释）。
pub const ABOUT_EGG_HARD_TIMEOUT: Duration = Duration::from_secs(4);

/// 关于页 logo 的静态显示边长（逻辑像素）
pub const ABOUT_LOGO_SIZE: f32 = 64.0;

/// 飞行/旋转期占位方盒边长（逻辑像素）。
///
/// 必须 ≥ 旋转 AABB 峰值（约 `ABOUT_LOGO_SIZE × 1.31`，非正方形 logo 实测），
/// 否则旋转时图标会被方盒裁掉观感上的边角。
pub const ABOUT_LOGO_BOX: f32 = 96.0;

/// 晃动振幅（逻辑像素，单侧最大值）
pub const ABOUT_LOGO_SHAKE_AMPLITUDE: f32 = 7.0;

/// 晃动时长（秒）
const SHAKE_DURATION: f32 = 0.30;
/// 晃动往复次数
const SHAKE_CYCLES: f32 = 2.0;
/// 快速转动时长（秒）
const SPIN_DURATION: f32 = 0.50;
/// 快速转动圈数
const SPIN_TURNS: f32 = 2.0;
/// 脱离原位（飞向随机落点）的时长（秒）
const DETACH_DURATION: f32 = 0.22;
/// 重力下落的名义时长（秒）：实际重力加速度按落点距离反解，保证时长稳定
const FALL_DURATION: f32 = 0.55;
/// 打入位停顿时长（秒）：音效挂载点，第二批内置音效后在此播放
const IMPACT_DURATION: f32 = 0.08;
/// 渐隐时长（秒）
const FADE_DURATION: f32 = 0.35;
/// 飞行范围与窗口四边的安全边距（逻辑像素）
const FLIGHT_MARGIN: f32 = 8.0;
/// logo 原始 `viewBox` 宽高比（`resources/icons/brands/app-logo.svg`：184.09878 × 218.6096），
/// 用于旋转期的 AABB 尺寸补偿
const LOGO_ASPECT_W: f32 = 184.1;
/// logo 原始 `viewBox` 高度（同上）
const LOGO_ASPECT_H: f32 = 218.6;

/// 进程级「关于页 logo 已消失」标志。
///
/// 需求：消失状态**仅在本次运行内保持**——重开设置面板不再出现、重启 APP 恢复。
/// 因此它是进程级 `static`（不写配置、不掉盘），与
/// `resources::icon::HIDPI_ENABLED` 同款先例。
static ABOUT_LOGO_VANISHED: AtomicBool = AtomicBool::new(false);

/// 关于页 logo 是否已在本进程内消失
#[must_use]
pub fn logo_has_vanished() -> bool {
    ABOUT_LOGO_VANISHED.load(Ordering::Relaxed)
}

/// 标记关于页 logo 已消失（仅进程内有效，重启自动恢复）
pub fn mark_logo_vanished() {
    ABOUT_LOGO_VANISHED.store(true, Ordering::Relaxed);
}

/// 复位「已消失」标志。
///
/// 仅供单元测试与显式复位路径使用；生产代码**不得**调用（否则违反
/// 「消失状态仅本次运行内保持」的约定）。
pub fn reset_logo_vanished() {
    ABOUT_LOGO_VANISHED.store(false, Ordering::Relaxed);
}

/// 关于页 logo 彩蛋的相位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EggPhase {
    /// 静止（关于页正常显示 logo）
    Idle,
    /// 单击反馈：原地晃动一次
    Shake,
    /// 彩蛋序列：快速转动（位置不变）
    Spin,
    /// 彩蛋序列：脱离原位飞向随机落点
    Detach,
    /// 彩蛋序列：重力加速砸向窗口底部
    Fall,
    /// 彩蛋序列：打入位停顿（音效挂载点）
    Impact,
    /// 彩蛋序列：渐隐消失
    FadeOut,
    /// 已消失（本次运行内不再出现）
    Vanished,
}

/// 关于页 logo 彩蛋状态机
#[derive(Debug, Clone)]
pub struct AboutEggState {
    /// 连点计数
    clicks: u32,
    /// 上一次点击时刻（超时清零判定用）
    last_click: Option<Instant>,
    /// 本次点击的指针位置（窗口逻辑像素；由 Host 在路由点击消息前写入）
    click_point: Option<(f32, f32)>,
    /// 当前相位
    phase: EggPhase,
    /// 当前相位已流逝时长（秒）
    phase_elapsed: f32,
    /// 彩蛋序列已流逝时长（秒，硬超时兜底用）
    sequence_elapsed: f32,
    /// 脱离原位的起点（方盒左上角，逻辑像素）
    origin: (f32, f32),
    /// 随机落点（方盒左上角，逻辑像素；中轴 Y 以上）
    target: (f32, f32),
    /// 当前方盒左上角位置（逻辑像素）
    position: (f32, f32),
    /// 重力加速度（像素/秒²，按落点距离在脱离结束时反解）
    gravity: f32,
    /// 下落速度（像素/秒）
    fall_velocity: f32,
    /// 当前旋转角（弧度）
    spin_angle: f32,
    /// 当前不透明度
    opacity: f32,
    /// 视口尺寸（窗口逻辑像素，由 Host 每帧注入）
    viewport: (f32, f32),
    /// 上一帧时刻（dt 计算）
    last_update: Option<Instant>,
    /// 随机数种子（xorshift64；避免为此引入 `rand` 依赖）
    seed: u64,
    /// 落地信号（进入 [`EggPhase::Impact`] 时置位，由调用方取走一次）
    ///
    /// 音效挂在**落地瞬间**，故用「置位 + 取走」表达一次性事件：状态机自身不依赖音频
    /// 设施（`ui-core` 不引入音频依赖），实际播放由 `lumino-ui` 消费本信号完成。
    impact_signal: bool,
}

impl Default for AboutEggState {
    fn default() -> Self {
        Self::new()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// 进程级标志使彩蛋用例之间存在共享状态：统一串行化，避免并行测试互相污染。
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 测试用构造：忽略进程级消失标志，保证用例可重复执行
    fn idle_state() -> AboutEggState {
        let mut state = AboutEggState::new();
        state.phase = EggPhase::Idle;
        state
    }

    /// 构造一个视口已注入、点击点已知的状态机
    fn ready_state() -> AboutEggState {
        let mut state = idle_state();
        state.set_viewport(720.0, 540.0);
        state.set_click_point(Some((305.0, 121.0)));
        state
    }

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn test_impact_signal_is_one_shot_at_landing() {
        let _guard = lock();
        let mut state = ready_state();
        let mut now = Instant::now();
        for _ in 0..ABOUT_EGG_CLICK_THRESHOLD {
            state.on_logo_click(now);
            now += ms(50);
        }

        // 起飞后、落地前不得有信号
        assert!(
            !state.take_impact_signal(),
            "序列开始前/落地前不应有落地信号"
        );

        let mut landed_at_impact = false;
        for _ in 0..600 {
            now += ms(16);
            let still = state.update(now);
            let signalled = state.take_impact_signal();
            if signalled {
                assert_eq!(
                    state.phase(),
                    EggPhase::Impact,
                    "落地信号必须与进入 Impact 相位同帧"
                );
                landed_at_impact = true;
                // 消费式：同一帧再取应为 false
                assert!(!state.take_impact_signal(), "落地信号必须只能取走一次");
            }
            if !still {
                break;
            }
        }
        assert!(landed_at_impact, "整段序列应产生恰好一次落地信号");
        assert!(!state.take_impact_signal(), "序列收尾后不应残留落地信号");
    }

    #[test]
    fn test_single_click_shakes_then_settles() {
        let _guard = lock();
        let mut state = ready_state();
        let t0 = Instant::now();

        assert!(state.on_logo_click(t0), "首次点击应被消费（触发晃动）");
        assert_eq!(state.phase(), EggPhase::Shake);
        assert_eq!(state.clicks(), 1);

        // 起振点为 0（不会在相位切换处跳变）
        assert!(state.shake_offset().abs() < f32::EPSILON);

        // 晃动全程位移不超振幅，且相位最终回到静止
        let mut now = t0;
        let mut peak = 0.0_f32;
        for _ in 0..40 {
            now += ms(16);
            state.update(now);
            peak = peak.max(state.shake_offset().abs());
            if state.phase() == EggPhase::Idle {
                break;
            }
        }
        assert_eq!(state.phase(), EggPhase::Idle, "晃动应在一个周期内收住");
        assert!(peak > 0.0, "晃动期间应产生可见位移");
        assert!(
            peak <= ABOUT_LOGO_SHAKE_AMPLITUDE,
            "晃动位移不得超振幅: {peak}"
        );
        assert_eq!(state.shake_offset(), 0.0, "静止相位位移恒为 0");
    }

    #[test]
    fn test_click_timeout_resets_counter() {
        let _guard = lock();
        let mut state = ready_state();
        let mut now = Instant::now();

        // 14 次连点（间隔 100ms，不超时）
        for _ in 0..14 {
            state.on_logo_click(now);
            now += ms(100);
        }
        assert_eq!(state.clicks(), 14);
        assert_eq!(state.phase(), EggPhase::Shake);

        // 停顿 3s（超过 2s 阈值）后再点：计数应从 1 重新累计
        now += Duration::from_secs(3);
        assert!(state.on_logo_click(now));
        assert_eq!(state.clicks(), 1, "超过超时阈值后计数应清零重新累计");
        assert!(!state.is_airborne(), "不应因未满阈值而触发彩蛋");
    }

    #[test]
    fn test_fifteen_clicks_trigger_full_sequence() {
        let _guard = lock();
        reset_logo_vanished();
        let mut state = ready_state();
        let mut now = Instant::now();

        for i in 0..ABOUT_EGG_CLICK_THRESHOLD {
            assert!(state.on_logo_click(now), "第 {} 次点击应被消费", i + 1);
            now += ms(100);
        }
        assert_eq!(state.phase(), EggPhase::Spin, "满阈值应进入快速转动");

        // 逐帧推进整段序列，记录相位出现顺序（含收尾的 Vanished）
        let mut order: Vec<EggPhase> = Vec::new();
        for _ in 0..600 {
            now += ms(16);
            let still_animating = state.update(now);
            if order.last() != Some(&state.phase()) {
                order.push(state.phase());
            }
            if !still_animating {
                break;
            }
        }

        assert_eq!(state.phase(), EggPhase::Vanished, "序列应完整播放至消失");
        assert_eq!(
            order,
            vec![
                EggPhase::Spin,
                EggPhase::Detach,
                EggPhase::Fall,
                EggPhase::Impact,
                EggPhase::FadeOut,
                EggPhase::Vanished,
            ],
            "相位顺序应为 转动→脱离→下落→打入位→渐隐→消失"
        );
        assert!(logo_has_vanished(), "序列结束应写入进程级消失标志");
        assert!(!state.is_visible_in_page());
        assert!(!state.is_airborne());
        assert!(!state.is_animating(), "消失后不得再要求逐帧驱动");
    }

    #[test]
    fn test_fall_target_is_above_window_center() {
        let _guard = lock();
        let mut state = ready_state();
        let mut now = Instant::now();
        for _ in 0..ABOUT_EGG_CLICK_THRESHOLD {
            state.on_logo_click(now);
            now += ms(50);
        }

        // 推进到下落开始的那一刻，检查脱离结束位置
        let mut checked = false;
        for _ in 0..200 {
            now += ms(16);
            if !state.update(now) {
                break;
            }
            if state.phase() == EggPhase::Fall {
                let (_, y) = state.position();
                assert!(
                    y + ABOUT_LOGO_BOX <= 540.0 * 0.5 + 0.5,
                    "脱离结束位置必须落在窗口中轴 Y 以上: y={y}"
                );
                checked = true;
                break;
            }
        }
        assert!(checked, "应在有限帧内进入下落相位");
    }

    #[test]
    fn test_fall_accelerates_monotonically() {
        let _guard = lock();
        let mut state = ready_state();
        let mut now = Instant::now();
        for _ in 0..ABOUT_EGG_CLICK_THRESHOLD {
            state.on_logo_click(now);
            now += ms(50);
        }

        let mut prev_y: Option<f32> = None;
        let mut prev_delta = 0.0_f32;
        let mut samples = 0;
        for _ in 0..200 {
            now += ms(16);
            if !state.update(now) {
                break;
            }
            if state.phase() != EggPhase::Fall {
                continue;
            }
            let (_, y) = state.position();
            if let Some(py) = prev_y {
                let delta = y - py;
                assert!(delta >= 0.0, "下落不应反向");
                if samples > 0 {
                    assert!(
                        delta >= prev_delta - 1e-3,
                        "自然加速：每帧位移应单调不减（第 {samples} 帧）"
                    );
                }
                prev_delta = delta;
                samples += 1;
            }
            prev_y = Some(y);
        }
        assert!(samples >= 3, "下落过程应至少采到 3 帧");
    }

    #[test]
    fn test_click_ignored_while_sequence_running() {
        let _guard = lock();
        let mut state = ready_state();
        let mut now = Instant::now();
        for _ in 0..ABOUT_EGG_CLICK_THRESHOLD {
            state.on_logo_click(now);
            now += ms(50);
        }
        assert_eq!(state.phase(), EggPhase::Spin);

        assert!(!state.on_logo_click(now), "序列进行中的点击应被忽略");
        assert_eq!(state.clicks(), 0, "序列进行中不得重新累计计数");
        assert_eq!(state.phase(), EggPhase::Spin, "序列相位不应被打断");
    }

    #[test]
    fn test_repeated_shake_does_not_trip_hard_timeout() {
        let _guard = lock();
        let mut state = ready_state();
        let start = Instant::now();
        let mut now = start;

        // 反复晃动（累计远超硬超时阈值），但始终未满 15 次
        for _ in 0..13 {
            state.on_logo_click(now);
            now += ms(500);
        }
        assert!(
            now.duration_since(start) > ABOUT_EGG_HARD_TIMEOUT,
            "构造条件：晃动累计时长已超过硬超时阈值"
        );

        // 再补 2 次触发序列：不应因为"晃动累计时长"被误判超时而立即收尾
        state.on_logo_click(now);
        now += ms(100);
        state.on_logo_click(now);
        assert_eq!(state.phase(), EggPhase::Spin);

        let mut reached_fall = false;
        for _ in 0..120 {
            now += ms(16);
            if !state.update(now) {
                break;
            }
            if state.phase() == EggPhase::Fall {
                reached_fall = true;
                break;
            }
        }
        assert!(reached_fall, "晃动累计时长不应触发序列硬超时兜底");
    }

    #[test]
    fn test_vanished_logo_absent_on_reopen_but_state_is_marked() {
        let _guard = lock();
        reset_logo_vanished();
        assert!(!logo_has_vanished());

        mark_logo_vanished();
        // 模拟「关掉设置面板 → 重新打开」：全新状态机应直接处于已消失
        let mut reopened = AboutEggState::new();
        assert_eq!(reopened.phase(), EggPhase::Vanished);
        assert!(!reopened.is_visible_in_page());
        assert!(
            !reopened.on_logo_click(Instant::now()),
            "消失后点击应被忽略"
        );

        reset_logo_vanished();
    }

    #[test]
    fn test_spin_fit_scale_compensates_rotated_bounding_box() {
        let _guard = lock();
        let mut state = idle_state();
        state.spin_angle = 0.0;
        assert!((state.spin_fit_scale() - 1.0).abs() < 1e-3);

        state.spin_angle = std::f32::consts::FRAC_PI_4;
        let at_45 = state.spin_fit_scale();
        assert!(
            (1.25..=1.35).contains(&at_45),
            "45° 时的尺寸补偿应约为 1.30，实际 {at_45}"
        );

        // 补偿后绘制尺寸恒定：Svg 固定边长 / AABB 长边 恒等于静止时的比例
        for step in 0..24 {
            state.spin_angle = std::f32::consts::TAU * step as f32 / 24.0;
            let (sin, cos) = (state.spin_angle.sin().abs(), state.spin_angle.cos().abs());
            let aabb = (LOGO_ASPECT_W * cos + LOGO_ASPECT_H * sin)
                .max(LOGO_ASPECT_W * sin + LOGO_ASPECT_H * cos);
            let fitted = ABOUT_LOGO_SIZE * state.spin_fit_scale();
            let drawn_height = LOGO_ASPECT_H * fitted / aabb;
            assert!(
                (drawn_height - ABOUT_LOGO_SIZE).abs() < 0.5,
                "旋转过程中绘制高度应恒定，实际 {drawn_height}"
            );
        }
    }

    #[test]
    fn test_position_stays_inside_viewport() {
        let _guard = lock();
        let mut state = ready_state();
        let mut now = Instant::now();
        for _ in 0..ABOUT_EGG_CLICK_THRESHOLD {
            state.on_logo_click(now);
            now += ms(50);
        }

        for _ in 0..600 {
            now += ms(16);
            if !state.update(now) {
                break;
            }
            let (x, y) = state.position();
            assert!(
                (-0.5..=720.0 - ABOUT_LOGO_BOX + 0.5).contains(&x),
                "X 越界: {x}"
            );
            assert!(
                (-0.5..=540.0 - ABOUT_LOGO_BOX + 0.5).contains(&y),
                "Y 越界: {y}"
            );
        }
    }
}
