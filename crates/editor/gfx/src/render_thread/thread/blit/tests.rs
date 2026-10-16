//! R0：blit 缓存双键判据测试（REND-019 PR-B F6，无 GPU 依赖）。
//!
//! 判据出处：实施范式 §〇 F6——静态缓存按 surface format 判重建、不感知
//! device 更换；不处理则重连后 present 用旧 device 的 pipeline 画新 device
//! 的纹理 → wgpu validation panic。

use super::blit_cache_needs_rebuild;
use crate::epoch::GpuEpoch;

/// 双键判据：空缓存 / 格式变化 / 代次变化都必须重建；同键命中。
#[test]
fn test_r0_blit_cache_needs_rebuild_dual_key() {
    let fmt = wgpu::TextureFormat::Bgra8UnormSrgb;
    let e0 = GpuEpoch::INITIAL;
    let e1 = e0.next();

    assert!(blit_cache_needs_rebuild(None, e0, fmt), "空缓存必须重建");
    assert!(
        !blit_cache_needs_rebuild(Some((e0, fmt)), e0, fmt),
        "同代次同格式必须命中"
    );
    assert!(
        blit_cache_needs_rebuild(Some((e0, fmt)), e0, wgpu::TextureFormat::Rgba8Unorm),
        "格式变化必须重建"
    );
    assert!(
        blit_cache_needs_rebuild(Some((e0, fmt)), e1, fmt),
        "代次变化（device 重建）必须重建——跨 device 混用资源是 validation error"
    );
}
