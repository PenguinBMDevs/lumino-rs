//! 无头 GPU 兼容性检测。
//!
//! 在无窗口、无事件循环的条件下，用真实卷帘渲染管线（[`crate::NoteRenderer`]）
//! 离屏绘制一帧并回读像素，判定本机 GPU 是否能正常走本平台要求的图形后端
//! （Windows/Linux = Vulkan，macOS = Metal）。
//!
//! 本模块不感知配置、设置与"首次启动"语义——只返回结构化 [`GpuCheckReport`]，
//! 由调用方（runner）决定是否检查、是否弹窗与如何持久化。
//! 检测使用独立 wgpu 实例，**不触碰** [`crate::Context`] 的进程级共享 GPU。
//!
//! 子模块：
//! - `report`：报告类型与平台后端策略
//! - `check`：检测入口与适配器枚举
//! - `timeout`：线程模型的超时包装与廉价探测（Windows 的完整检测改由宿主
//!   `device_probe` 子进程隔离，见 `src/device_probe/`）
//! - `render`：离屏试画与回读校验

mod check;
mod render;
mod report;
mod timeout;

pub use check::{FallbackDiagnostics, check_gpu_support, debug_force_fail_requested};
pub use report::{GpuAdapterSummary, GpuCheckFailure, GpuCheckReport, required_backend_name};
pub use timeout::{probe_adapter_fingerprints_with_timeout, run_check_with_timeout};

/// GPU 检测默认超时（独立线程 + `recv_timeout`，保证不阻塞启动）
pub const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// 缓存指纹廉价探测的超时（独立线程 + `recv_timeout`）。
///
/// 语义：**探测超时 ≠ 检测失败**——只说明缓存无法快速校验，调用方按 cache miss
/// 落回 [`run_check_with_timeout`] 全量检测（见 `device_gate`）。
///
/// 正常机器上探测通常仅需几十毫秒，1.5 秒为异常驱动留足余量；最坏启动阻塞 =
/// 探测超时 + 全量超时 ≈ 4.5 秒，取代 UI-008 / #37 修复前的「无限冻结」。
pub const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

#[cfg(test)]
mod tests;
