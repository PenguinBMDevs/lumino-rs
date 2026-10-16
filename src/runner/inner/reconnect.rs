//! REND-019 PR-C：Runner 的重连驱动与操作面（`ReconnectOps<SharedGpu>` 实现）。
//!
//! 驱动链：`UserEvent::GpuDeviceLost` → [`Runner::on_gpu_device_lost`]（发起会话）
//! → [`Runner::step_reconnect`]（`about_to_wait` 每帧一步）→ 各操作经本文件
//! 落到具体窗口/线程/全局状态。

use std::sync::atomic::Ordering;
use std::sync::mpsc;

use lumino_gfx::{GpuEpoch, SharedGpu};

use super::Runner;
use crate::runner::reconnect::{ReconnectOps, ReconnectSession};

impl Runner {
    /// device-lost 事件入口（§1.3 C1：重连决策只由 UI 主循环发起）。
    pub(crate) fn on_gpu_device_lost(&mut self, reason: &str, message: &str) {
        tracing::error!("收到 GpuDeviceLost 用户事件：{reason} — {message}");
        if self.inner.is_none() {
            tracing::warn!("初始化未完成，无法重连（无窗口基础设施可重建）");
            return;
        }
        if self.reconnect.is_some() {
            tracing::warn!("已有重连会话进行中，忽略重复的 device-lost 事件");
            return;
        }
        // 状态条提示（R2 验收点：「GPU 重连中…」可见性）
        if let Some(this) = self.inner.as_mut() {
            this.window_state.window.ui_mut().set_status_message(Some(
                lumino_core::i18n::tr("GPU 重连中…（驱动超时恢复）").to_string(),
            ));
        }
        self.reconnect = Some(ReconnectSession::new());
        tracing::info!("重连会话启动（REND-019 PR-C：epoch 将递增并重绑全部窗口）");
    }

    /// 每帧推进重连流水线（`about_to_wait` 调用；每帧最多一步，§1.4 S1）。
    pub(crate) fn step_reconnect(&mut self) {
        let Some(mut session) = self.reconnect.take() else {
            return;
        };
        session.step(self);
        if session.is_finished() {
            // 成功由 on_recovered 收尾（状态条撤销）；失败由 on_failed 收尾（已弹兜底框）
            tracing::info!("重连会话结束：{}", session.outcome_label());
        } else {
            self.reconnect = Some(session);
        }
    }
}

impl ReconnectOps<SharedGpu> for Runner {
    fn stop_render_threads(&mut self) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };
        // 视频导出进行中：置取消标志（导出线程以明确错误收场；细粒度收口属 PR-D）
        if this
            .window_state
            .video_export_running
            .load(Ordering::SeqCst)
        {
            tracing::warn!("重连：视频导出进行中，置取消标志");
            this.window_state
                .video_export_cancel
                .store(true, Ordering::SeqCst);
        }
        this.window_state
            .window
            .ui_mut()
            .stop_render_thread_for_reconnect();
    }

    fn spawn_device_recreate(&mut self) -> mpsc::Receiver<Result<(GpuEpoch, SharedGpu), String>> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(lumino_gfx::recreate_shared_gpu_worker());
        });
        tracing::info!("重连：设备重建 worker 已发起（异步，逐帧收割）");
        rx
    }

    fn install_and_rebuild_infra(&mut self, epoch: GpuEpoch, gpu: SharedGpu) -> Result<(), String> {
        // 1) 安装新共享状态（epoch 单调递增；后续 gpu_snapshot 均见新代次）
        lumino_gfx::install_recreated_shared_gpu(epoch, gpu)?;
        let (epoch, gpu) = lumino_gfx::gpu_snapshot().map_err(|e| e.to_string())?;

        // 2) 设备检查警告窗（该阶段通常不存在；防御性覆盖）
        if let Some(warning) = self.device_warning.as_mut() {
            warning.rebind_gpu(epoch, &gpu);
        }

        // 3) 全部存活窗口（结构即注册表：主窗 + 对话框（含初始化中）+ 进度窗 +
        //    云进度窗——单一所有权下逐字段遍历，天然含"新建登记 / 销毁缺席"语义）
        let Some(this) = self.inner.as_mut() else {
            return Err("RunnerInner 不可用".to_string());
        };
        this.window_state.window.rebind_gpu(epoch, &gpu);
        let dialog_count = this.window_state.dialog_manager.rebind_gpu_all(epoch, &gpu);
        this.window_state.progress.rebind_gpu(epoch, &gpu);
        this.window_state.cloud_progress.rebind_gpu(epoch, &gpu);
        tracing::info!(
            "重连：基础设施重建完成（主窗 + {dialog_count} 对话框 + 进度/云进度窗），epoch = {}",
            epoch.0
        );
        Ok(())
    }

    fn mark_replay(&mut self) -> Result<(), String> {
        let this = self.inner.as_mut().ok_or("RunnerInner 不可用")?;
        // 洋葱皮走既有流式路径从文档全量重建（REND-003）
        this.window_state.window.ui_mut().mark_gpu_replay();
        tracing::info!("重连：数据回放标记完成（下一次决策强制全量重建）");
        Ok(())
    }

    fn rebuild_iced(&mut self) -> Result<(), String> {
        // iced Engine/Renderer 重建已并入各窗口 rebind 流程
        // （render_ctx::rebind_gpu）；保留独立步骤位以对齐 §1.4 状态机结构。
        Ok(())
    }

    fn restart_render_threads(&mut self) -> Result<(), String> {
        // 撤销停帧标志（全部窗口已重绑到新 device，渲染可安全恢复）
        lumino_gfx::clear_device_lost();
        let this = self.inner.as_mut().ok_or("RunnerInner 不可用")?;
        this.window_state
            .window
            .ui_mut()
            .restart_render_thread_for_reconnect();
        this.window_state.window.window().request_redraw();
        Ok(())
    }

    fn on_recovered(&mut self) {
        tracing::info!("GPU 重连完成：渲染已恢复");
        if let Some(this) = self.inner.as_mut() {
            this.window_state.window.ui_mut().set_status_message(None);
        }
    }

    fn on_failed(&mut self, reason: &str) {
        tracing::error!("GPU 重连失败（进入兜底）：{reason}");
        // 红线 9：Failed 兜底态触发前必须先完成自动保存
        let saved = self
            .inner
            .as_mut()
            .and_then(|this| this.auto_save_before_failed());
        if let Some(this) = self.inner.as_mut() {
            let msg = match &saved {
                Some(path) => format!(
                    "{}（工程已自动保存到 {}）",
                    lumino_core::i18n::tr("GPU 重连失败：请重启 Lumino"),
                    path.display()
                ),
                None => lumino_core::i18n::tr("GPU 重连失败：请重启 Lumino").to_string(),
            };
            this.window_state
                .window
                .ui_mut()
                .set_status_message(Some(msg));
        }
        // 终态兜底提示（阻塞式；用户确认后自行重启——PR-C 无进程内二次恢复路径）
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Lumino")
            .set_description(format!(
                "{reason}\n\n{}",
                lumino_core::i18n::tr("GPU 重连失败：请重启 Lumino")
            ))
            .show();
    }
}
