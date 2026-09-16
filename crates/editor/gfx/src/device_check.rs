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
//! - `check`：检测入口、超时封装与适配器枚举
//! - `render`：离屏试画与回读校验

mod check;
mod render;
mod report;

pub use check::{
    check_gpu_support, debug_force_fail_requested, probe_adapter_fingerprints,
    run_check_with_timeout,
};
pub use report::{GpuAdapterSummary, GpuCheckFailure, GpuCheckReport, required_backend_name};

/// GPU 检测默认超时（独立线程 + `recv_timeout`，保证不阻塞启动）
pub const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

#[cfg(test)]
mod tests;
