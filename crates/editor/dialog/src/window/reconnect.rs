//! REND-019 PR-C：对话框窗口重连重绑。

use lumino_gfx::{GpuEpoch, SharedGpu};

use super::DialogWindow;

impl DialogWindow {
    /// 重连：把本窗口的 GFX 上下文与 Host 重绑到新 device。
    ///
    /// 覆盖分帧初始化中的窗口（`gfx` 已建但 `ui` 未建）——两者各自判存重绑；
    /// 尚未建 GFX 的窗口跳过：其后续 `initialize_gfx` 会从最新快照建新 device。
    pub(crate) fn rebind_gpu(&mut self, epoch: GpuEpoch, gpu: &SharedGpu) {
        if let Some(gfx) = self.gfx.as_mut() {
            gfx.rebind_device(epoch, gpu);
        }
        if let Some(ui) = self.ui.as_mut() {
            ui.rebind_gpu(epoch, gpu);
        }
    }
}
