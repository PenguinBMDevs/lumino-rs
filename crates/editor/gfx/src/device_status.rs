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

use std::sync::OnceLock;
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
///
/// # 返回值
/// `true` = 本次调用完成置位（首置）；`false` = 此前已置位。
/// REND-019（R2 实测）：停帧窗口内重绘循环每帧都会走 Stop 处置，
/// 调用方可据此把伴随日志收敛为「仅首次告警」。
#[must_use]
pub fn mark_device_lost() -> bool {
    let first = !DEVICE_LOST.swap(true, Ordering::SeqCst);
    if first {
        tracing::error!("DEVICE_LOST 置位：停止渲染，等待重连接管（REND-019 PR-A）");
    }
    first
}

/// 撤销停帧标志（REND-019 PR-C：重连基础设施重建完成后恢复渲染）。
///
/// 仅重连流水线（UI 主循环）在**全部窗口重绑到新 device 之后**允许调用；
/// 置位路径（回调 / 帧错误）不得调用（红线 C1：重连决策只由 UI 主循环发起）。
pub fn clear_device_lost() {
    if DEVICE_LOST.swap(false, Ordering::SeqCst) {
        tracing::info!("DEVICE_LOST 撤销：重连完成，恢复渲染（REND-019 PR-C）");
    }
}

/// UI 通知器签名（REND-019 PR-C §1.3 C1/C2）。
type LostNotifier = Box<dyn Fn(wgpu::DeviceLostReason, String) + Send + Sync>;

/// 进程级 UI 通知器（仅允许注册一次；未注册时回调只置标志）。
static LOST_NOTIFIER: OnceLock<LostNotifier> = OnceLock::new();

/// 注册 device-lost 的 UI 通知器（REND-019 PR-C）。
///
/// 由根 crate 在事件循环创建时注册（如 `EventLoopProxy::send_event` 无锁唤醒）。
///
/// 约束（红线 C1）：通知器在 wgpu 回调线程执行——只允许无锁、非阻塞动作；
/// 禁止任何 wgpu 调用 / 锁获取 / 阻塞发送 / panic。
pub fn set_device_lost_notifier(
    notifier: impl Fn(wgpu::DeviceLostReason, String) + Send + Sync + 'static,
) {
    let _ = LOST_NOTIFIER.set(Box::new(notifier));
}

/// device-lost 回调的正式实现（`set_device_lost_callback` 接线目标）。
///
/// 红线 C1：回调线程不受控，只允许日志 + 原子标志写 + 无锁事件唤醒——
/// 禁止任何 wgpu 调用 / 锁获取 / 阻塞发送 / panic。
pub fn on_device_lost(reason: wgpu::DeviceLostReason, message: String) {
    tracing::error!("wgpu device lost: {reason:?} — {message}");
    let _ = mark_device_lost();
    // §1.3 C1/C2：通知 UI 主循环（无锁 send_event）；重连决策只由 UI 主循环发起
    if let Some(notify) = LOST_NOTIFIER.get() {
        notify(reason, message);
    }
}

/// 仅供测试：复位标志（进程级静态，避免测试间串扰）。
#[cfg(test)]
pub(crate) fn reset_for_test() {
    DEVICE_LOST.store(false, Ordering::SeqCst);
}

/// R2 测试注入（调试特性 `gpu-reconnect-test`，REND-019 §三 R2）。
///
/// env `LUMINO_TEST_DEVICE_LOST`（非 "0" 值）启用：延时后对**当前共享 device**
/// 执行 `destroy()`——与真实 TDR 走完全相同的 device-lost 回调路径。
///
/// R2 实测（2026-10-17 第 2 轮卡死）后改为**安全点注入**：注入线程只置位
/// [`INJECTION_PENDING`]，实际 `destroy()` 由 UI 线程在 `Context::with_frame`
/// 帧间静态点执行（见该处注释）——避免注入线程与在途 surface 调用
/// （acquire / present）驱动级竞态导致的 UI 线程永久阻塞。
///
/// env：
/// - `LUMINO_TEST_DEVICE_LOST_DELAY`：首次注入延时秒数（默认 5）；
/// - `LUMINO_TEST_DEVICE_LOST_INTERVAL`：重复注入间隔秒数（0/缺省 = 单次）。
#[cfg(feature = "gpu-reconnect-test")]
static INJECTION_PENDING: AtomicBool = AtomicBool::new(false);

/// R2 注入：取走待执行请求（`Context::with_frame` 帧安全点调用）。
#[cfg(feature = "gpu-reconnect-test")]
pub(crate) fn take_injection_request() -> bool {
    INJECTION_PENDING.swap(false, Ordering::SeqCst)
}

/// 默认构建：空操作（注入仅存在于 `gpu-reconnect-test` 调试特性）。
#[cfg(not(feature = "gpu-reconnect-test"))]
pub(crate) fn take_injection_request() -> bool {
    false
}

#[cfg(feature = "gpu-reconnect-test")]
pub(crate) fn maybe_spawn_device_lost_injection() {
    static SPAWNED: AtomicBool = AtomicBool::new(false);
    if SPAWNED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(value) = std::env::var_os("LUMINO_TEST_DEVICE_LOST") else {
        return;
    };
    if value.to_string_lossy() == "0" {
        return;
    }
    let delay = env_secs("LUMINO_TEST_DEVICE_LOST_DELAY", 5);
    let interval = env_secs("LUMINO_TEST_DEVICE_LOST_INTERVAL", 0);
    tracing::warn!(
        "[gpu-reconnect-test] 注入已武装：{delay}s 后请求 destroy 共享 device（interval={interval}s）"
    );
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(delay));
        loop {
            INJECTION_PENDING.store(true, Ordering::SeqCst);
            tracing::warn!(
                "[gpu-reconnect-test] 注入已请求：等待 UI 帧安全点执行 device.destroy()（R2，与真实 TDR 同回调路径）"
            );
            if interval == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(interval));
        }
    });
}

#[cfg(feature = "gpu-reconnect-test")]
fn env_secs(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// 默认构建：空操作（注入仅存在于 `gpu-reconnect-test` 调试特性）。
#[cfg(not(feature = "gpu-reconnect-test"))]
pub(crate) fn maybe_spawn_device_lost_injection() {}

/// R1（wgpu 级注入）测试：独立设备 `destroy()` → 回调 → 标志链路。
#[cfg(test)]
#[path = "device_status/tests.rs"]
mod tests;
