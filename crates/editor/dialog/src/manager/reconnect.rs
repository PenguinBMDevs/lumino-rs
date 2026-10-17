//! REND-019 PR-C：对话框重连重绑（DialogManager 级）。

use lumino_gfx::{GpuEpoch, SharedGpu};

use super::DialogManager;

impl DialogManager {
    /// 重连：把所有活跃 + 分帧初始化中的对话框重绑到新 device；返回重绑数量。
    ///
    /// 结构即注册表：`dialogs`（已就绪）+ `initializing`（分帧初始化中）两个
    /// 容器即全部存活对话框——新建窗口天然入列、销毁窗口天然缺席。
    pub fn rebind_gpu_all(&mut self, epoch: GpuEpoch, gpu: &SharedGpu) -> usize {
        tracing::info!(
            "重连：对话框重绑列表 = {} 活跃 + {} 初始化中",
            self.dialogs.len(),
            self.initializing.len()
        );
        let mut count = 0usize;
        for window in self.dialogs.values_mut() {
            window.rebind_gpu(epoch, gpu);
            count += 1;
        }
        for (window, _) in self.initializing.iter_mut() {
            window.rebind_gpu(epoch, gpu);
            count += 1;
        }
        count
    }
}
