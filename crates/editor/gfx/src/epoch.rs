//! GPU 代次与跨帧资源登记（REND-019 PR-B epoch 地基）。
//!
//! 唯一权威类型定义处（评审依据：实施范式 §1.1/§1.2）：
//! - [`GpuEpoch`]：每次 device 重建单调递增，是一切"不可复用"判据；
//!   禁止直接用 `u64` 传代次——类型隔离防止与 `onion_epoch` 等内容代次混用。
//! - [`EpochTagged`]：所有跨帧存活的 GPU 句柄的登记包装；使用点唯一入口
//!   [`EpochTagged::get`]，代次不符返回 `None`，调用方**必须**走既有重建
//!   路径（E2），禁止复用死资源（E3）。
//!
//! PR-B 阶段行为不变：代次恒为 [`GpuEpoch::INITIAL`]，比对永远命中；
//! PR-C 的重连流水线在 device 重建时递增代次，本模块即开始生效。

/// GPU 代次。每次 device 重建单调递增，是一切不可复用判据。
///
/// 禁止直接用 u64 传代次——类型隔离防止与 `onion_epoch` 等内容代次混用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GpuEpoch(pub u64);

impl GpuEpoch {
    /// 初始代次（进程首个 device）。
    pub const INITIAL: Self = Self(0);

    /// 下一个个代（device 重建时由重连流水线调用；PR-B 阶段无调用点）。
    ///
    /// 饱和递增：极端情况下不溢出回绕（回绕会让旧资源"误命中"新代次）。
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// 跨帧 GPU 资源登记包装。
///
/// `T` 通常是 `Arc<TextureView>` / `RenderPipeline` / 渲染器等跨帧句柄。
/// 所有跨帧存活的 GPU 句柄必须包一层（E1），每帧使用前比对当前代次（G2）。
pub struct EpochTagged<T> {
    epoch: GpuEpoch,
    inner: T,
}

impl<T> EpochTagged<T> {
    /// 以创建时的代次登记。
    pub fn new(epoch: GpuEpoch, inner: T) -> Self {
        Self { epoch, inner }
    }

    /// 登记时的代次。
    #[must_use]
    pub fn epoch(&self) -> GpuEpoch {
        self.epoch
    }

    /// 使用点唯一入口：代次不符返回 `None`。
    ///
    /// 调用方**必须**走既有重建路径（E2），禁止"尝试继续使用"（E3：
    /// wgpu 27 对跨 device 混用资源是 validation error，不是可侥幸的未定义行为）。
    #[must_use]
    pub fn get(&self, current: GpuEpoch) -> Option<&T> {
        (self.epoch == current).then_some(&self.inner)
    }

    /// 消费包装取回内层值（丢弃路径：代次不符时由调用方 drop + 惰性重建）。
    #[must_use]
    pub fn into_inner(self) -> T {
        self.inner
    }
}

/// R0：EpochTagged 跨代次判据测试（独立文件，保持本文件 < 400 行）。
#[cfg(test)]
#[path = "epoch/tests.rs"]
mod tests;
