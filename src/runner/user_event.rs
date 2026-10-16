//! REND-019 PR-C：winit UserEvent（§1.3 C2：事件循环参数化）。
//!
//! 用途：
//! - `UiWake`：通用 UI 唤醒（原 `send_event(())` 负载）；
//! - `GpuDeviceLost`：device-lost 回调经 `EventLoopProxy` 无锁投递，
//!   由 UI 主循环消费后发起重连（C1：重连决策只在主循环）。
//!
//! `reason` 以字符串承载（回调侧 `format!("{reason:?}")`）——避免根 crate
//! 直接依赖 wgpu 类型；语义（Destroyed / 真实 TDR）在日志与检查单中对照。

/// 应用级用户事件（winit 事件循环参数化）。
#[derive(Debug)]
pub enum UserEvent {
    /// 通用 UI 唤醒（无负载；UI 事件队列由 `about_to_wait` 消费）
    UiWake,
    /// GPU device lost（reason / message 来自 wgpu 回调线程）
    GpuDeviceLost {
        /// `DeviceLostReason` 的 Debug 文本（如 `Unknown` / `Destroyed`）
        reason: String,
        /// 驱动提供的说明文本
        message: String,
    },
}
