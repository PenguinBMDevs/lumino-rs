//! 设备丢失 / 停帧状态（REND-019 PR-A 先行防护）。
//!
//! 全局标志语义：置位 =「停止渲染，等待重连」。置位来源：
//! - `SharedGpu` 的 device-lost 回调（TDR / 驱动复位）——见 [`on_device_lost`]，
//!   由 `Context` 初始化时接线；回调内只允许日志 + 原子写（红线 C1）；
//! - `Context::handle_frame_error` 遇 `SurfaceError::Lost / OutOfMemory`。
//!
//! 消费方：
//! - 渲染线程主循环每轮首行检查，置位则有序退出（不再对死设备发命令）；
//! - UI 侧 blit 前复查（`host/render/separate_thread/core.rs`），置位则本帧放弃拷贝；
//! - `handle_frame_error` 返回 `FrameErrorAction::Stop`，调用点不再盲重绘。
//!
//! 演进说明（REND-019 计划范式）：
//! - PR-B：本标志并入 `GpuEpoch` / `SharedGpuState` 快照体系；
//! - PR-C：回调增加 `UserEvent::GpuDeviceLost` 事件循环通知 + 重连状态机。
//! 本模块只做「止血」最小闭环：置位后不崩溃、不盲重试、等待上层接管。

use std::sync::atomic::{AtomicBool, Ordering};

/// 全局停帧标志（置位后不再对设备发起渲染 / 呈现）。
///
/// `SeqCst`：回调线程写入、渲染线程 / UI 线程读取，需要全序可见性。
static DEVICE_LOST: AtomicBool = AtomicBool::new(false);

/// 设备丢失 / 停帧标志是否已置位。
#[must_use]
pub fn device_lost() -> bool {
    DEVICE_LOST.load(Ordering::SeqCst)
}

/// 置位停帧标志（幂等；仅首次置位打日志，避免重复刷屏）。
///
/// 可从任意线程调用（含 wgpu 回调线程）——只做一次原子写 + 日志。
pub fn mark_device_lost() {
    if !DEVICE_LOST.swap(true, Ordering::SeqCst) {
        tracing::error!("DEVICE_LOST 置位：停止渲染，等待重连接管（REND-019 PR-A）");
    }
}

/// device-lost 回调的正式实现（`set_device_lost_callback` 接线目标）。
///
/// 红线 C1：回调线程不受控，只允许日志 + 原子标志写——
/// 禁止任何 wgpu 调用 / 锁获取 / 阻塞发送 / panic。
pub fn on_device_lost(reason: wgpu::DeviceLostReason, message: String) {
    tracing::error!("wgpu device lost: {reason:?} — {message}");
    mark_device_lost();
}

/// 仅供测试：复位标志（进程级静态，避免测试间串扰）。
#[cfg(test)]
pub(crate) fn reset_for_test() {
    DEVICE_LOST.store(false, Ordering::SeqCst);
}

/// R1（wgpu 级注入）测试：独立设备 `destroy()` → 回调 → 标志链路。
#[cfg(test)]
#[path = "device_status/tests.rs"]
mod tests;
