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

mod anim;

#[cfg(test)]
mod tests;
