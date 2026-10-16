//! REND-019 PR-C：重连接入（Host 级）。
//!
//! 重连流水线由根 crate 状态机驱动（§1.4），本文件提供 Host 侧四类操作：
//! 停止渲染线程 / 重绑到新 device / 数据回放标记 / 重启渲染线程。
//! 逻辑全部复用既有重建路径（E2），不发明"资源迁移/复活"。

use lumino_gfx::{GpuEpoch, SharedGpu};

use super::Host;

impl Host {
    /// 重连：停止分离渲染线程（幂等；无线程时 no-op）。
    ///
    /// PR-A 起渲染线程在 `DEVICE_LOST` 置位后已自行有序退出，此处 join 预期即时。
    pub fn stop_render_thread_for_reconnect(&mut self) {
        self.disable_separate_render_thread();
    }

    /// 重连：把本窗口重绑到新 device（见 `RenderContext::rebind_gpu`）。
    pub fn rebind_gpu(&mut self, epoch: GpuEpoch, gpu: &SharedGpu) {
        self.render_ctx
            .rebind_gpu(epoch, gpu, &self.window_ctx.window);
        // REND-019：重绑后强制下一帧全量重建 UI（渲染器/树缓存均已换代）
        self.ui_dirty = true;
    }

    /// 重连：数据回放标记——洋葱皮强制全量重建（数据从文档重放）+ 内容脏。
    pub fn mark_gpu_replay(&mut self) {
        self.mark_onion_replay();
        self.render_ctx.mark_render_content_dirty();
    }

    /// 重连：重启分离渲染线程（新 device/epoch 下创建全新线程与渲染器集合）。
    pub fn restart_render_thread_for_reconnect(&mut self) {
        self.enable_separate_render_thread();
    }
}
