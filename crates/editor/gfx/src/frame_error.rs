//! 帧错误统一收口（REND-019 PR-A §1.6）。
//!
//! 5 处 `with_frame` 调用点（主窗口 / 对话框 / 进度窗 / 云进度 / 设备警告窗）
//! 此前一律对 `Err` 盲 `request_redraw` 重试；本模块提供分类与动作语义，
//! `Context::handle_frame_error` 执行对应恢复动作（Outdated 重配交换链 /
//! Lost·OutOfMemory 置停帧标志）。
//!
//! 分类表（四种 `wgpu::SurfaceError`，R0 表驱动测试目标）：
//!
//! | 错误 | 语义 | 动作 |
//! |---|---|---|
//! | `Outdated` | 交换链过期（尺寸 / 格式变化） | `Reconfigured`：已重配，安排一次重绘 |
//! | `Timeout` | GPU 忙，取帧超时 | `SkipFrame`：本帧放弃，下帧重试 |
//! | `Lost` | Surface 丢失（设备丢失 / 失效） | `Stop`：置停帧标志等重连，禁止盲重绘 |
//! | `OutOfMemory` | 显存不足以支撑交换链 | `Stop`：同上 |
//! | `Other` | 未归类的瞬时错误 | `SkipFrame`：按可恢复处理，下帧重试（重绘事件驱动，无忙循环） |

use wgpu::SurfaceError;

/// 帧错误处置动作（[`crate::Context::handle_frame_error`] 返回值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameErrorAction {
    /// `Outdated`：已按当前配置重新 configure 交换链，安排一次重绘即可恢复。
    Reconfigured,
    /// `Timeout`：GPU 忙，本帧放弃；安排下一帧重试。
    SkipFrame,
    /// `Lost` / `OutOfMemory`：已置停帧标志（等重连）；禁止再盲重绘。
    Stop,
}

impl FrameErrorAction {
    /// 处置后是否应安排下一次重绘。
    ///
    /// `Stop`（设备丢失 / 交换链 OOM）置位停帧标志等待重连——此时
    /// `request_redraw` 会形成「重绘 → 失败 → 重绘」的忙循环，必须禁止；
    /// 其余两种错误重绘一次即可恢复推进。
    #[must_use]
    pub fn should_redraw(self) -> bool {
        !matches!(self, Self::Stop)
    }
}

/// 纯分类函数（无 GPU 依赖）：`wgpu::SurfaceError` → 处置动作。
#[must_use]
pub fn classify_frame_error(err: &SurfaceError) -> FrameErrorAction {
    match err {
        SurfaceError::Outdated => FrameErrorAction::Reconfigured,
        SurfaceError::Timeout => FrameErrorAction::SkipFrame,
        SurfaceError::Lost | SurfaceError::OutOfMemory => FrameErrorAction::Stop,
        // `Other`（wgpu 27 的未归类变体）：按瞬时错误处理，下帧重试。
        // 重绘由窗口事件驱动，不存在忙循环风险。
        SurfaceError::Other => FrameErrorAction::SkipFrame,
    }
}

/// R0：分类表驱动测试（独立文件，保持本文件 < 400 行）。
#[cfg(test)]
#[path = "frame_error/tests.rs"]
mod tests;
