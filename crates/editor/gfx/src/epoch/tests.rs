//! R0：`GpuEpoch` / `EpochTagged` 判据测试（无 GPU 依赖）。

use super::{EpochTagged, GpuEpoch};

/// R0 核心判据：同代次命中、跨代次必 `None`（E3：禁止复用死资源）。
#[test]
fn test_r0_epoch_tagged_cross_epoch_returns_none() {
    let tagged = EpochTagged::new(GpuEpoch::INITIAL, "texture-view#1");

    // 同代次命中
    assert_eq!(
        tagged.get(GpuEpoch::INITIAL),
        Some(&"texture-view#1"),
        "同代次必须命中"
    );
    // 跨代次必 None（device 重建后的旧资源不可复用）
    assert_eq!(
        tagged.get(GpuEpoch::INITIAL.next()),
        None,
        "跨代次必须返回 None"
    );
    assert_eq!(tagged.get(GpuEpoch(42)), None, "任意异代次必须返回 None");
}

/// 登记代次可回读；`into_inner` 供丢弃/重建路径取回句柄。
#[test]
fn test_r0_epoch_tagged_epoch_and_into_inner() {
    let tagged = EpochTagged::new(GpuEpoch(7), vec![1u8, 2, 3]);
    assert_eq!(tagged.epoch(), GpuEpoch(7));
    assert_eq!(tagged.into_inner(), vec![1u8, 2, 3]);
}

/// 代次单调递增（device 重建时依赖单调性排除"回绕误命中"）。
#[test]
fn test_r0_gpu_epoch_monotonic_next() {
    let a = GpuEpoch::INITIAL;
    let b = a.next();
    let c = b.next();
    assert!(a < b && b < c, "代次必须严格单调递增");
    assert_eq!(GpuEpoch(u64::MAX).next(), GpuEpoch(u64::MAX), "饱和不回绕");
}
