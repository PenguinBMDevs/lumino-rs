//! Runner 侧视频导出入口的 CPU 洋葱皮守卫（Step 4b 最终兜底）
//!
//! # 为什么还要在 Runner 再拦一次
//!
//! 视频导出有两套 Root：
//! - **主窗口 Root**（内嵌导出面板 + 菜单的 `OpenVideoExportDialog`）：
//!   `hires.backend` 与运行时真源一致，入口闸可靠；
//! - **视频导出独立对话框 Root**：`hires.backend` 来自创建对话框时的 `UiConfig`，
//!   可能与运行时真源存在时序差（对话框先建、后端后改）。
//!
//! 对话框侧的入口闸只能**尽力而为**。Runner 持有主窗口 `Host`，
//! `Host::onion_skin_backend()` 是运行时唯一真源，因此这里覆盖两条关键路径：
//! 1. 菜单 `OpenVideoExportDialog`——CPU 下不必开窗，开了也只是个「开始导出即失败」的窗；
//! 2. 事件 `StartVideoExport`——对话框/快捷键绕过 Root 入口闸时的最后一道闸。
//!
//! # 行为
//!
//! - GPU 后端：**恒返回 `false` 且零副作用**，行为与此前完全一致；
//! - CPU 后端：主窗口 Root 弹 `Warning` Toast（用户一定能看见），返回 `true`
//!   让调用方立即 return——不置状态、不发事件、不启动后台渲染线程。

use lumino_core::storage::config::OnionSkinBackend;
use lumino_core::tr;
use lumino_ui::toast::ToastLevel;

use crate::runner::RunnerInner;

/// 该后端是否禁止视频导出入口（纯函数，便于单测与未来新增后端时集中扩展）。
pub(crate) fn blocks_video_export(backend: OnionSkinBackend) -> bool {
    backend == OnionSkinBackend::Cpu
}

impl RunnerInner {
    /// CPU 洋葱皮后端下拒绝视频导出入口：Toast + 返回 `true`（调用方立即 return）。
    ///
    /// 返回 `true` 表示**已拦截**；`false` 表示放行。
    pub(crate) fn reject_video_export_on_cpu(&mut self) -> bool {
        let backend = self.window_state.window.ui().onion_skin_backend();
        if !blocks_video_export(backend) {
            return false;
        }
        tracing::warn!("CPU 洋葱皮后端：拒绝视频导出入口（GPU 专属逐帧渲染链路）");
        self.window_state
            .window
            .ui_mut()
            .root_mut()
            .toast
            .push(ToastLevel::Warning, tr!("CPU 洋葱皮后端不支持视频导出"));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::blocks_video_export;
    use lumino_core::storage::config::OnionSkinBackend;

    /// 默认后端（GPU）必须完全不受影响——这是「GPU 行为不变」的最低保证。
    #[test]
    fn test_gpu_backend_does_not_block_video_export() {
        assert!(!blocks_video_export(OnionSkinBackend::Gpu));
    }

    #[test]
    fn test_cpu_backend_blocks_video_export() {
        assert!(blocks_video_export(OnionSkinBackend::Cpu));
    }
}
