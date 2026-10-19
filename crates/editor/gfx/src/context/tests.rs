//! `context` 模块回归测试（与主文件分离，保持 `context.rs` 行数可控）。

use super::surface_usage;

/// 交换链纹理用途必须同时包含 `RENDER_ATTACHMENT` 与 `COPY_DST`。
///
/// 回归背景（2026-10-19 Android 真机）：为对齐探针把 usage 收窄成仅
/// `RENDER_ATTACHMENT` 后，`copy_offscreen_to_surface`（离屏纹理 → 交换链的
/// `copy_texture_to_texture`，见 `render_thread/thread/blit.rs`）被 wgpu 校验
/// 拒绝。用户可见现象是「UI 框架/菜单正常，**钢琴卷帘整块黑**」——因为音符区
/// 走离屏合成，而 UI 框架直接画在交换链上。
#[test]
fn surface_usage_keeps_copy_dst() {
    let usage = surface_usage();

    assert!(
        usage.contains(wgpu::TextureUsages::COPY_DST),
        "交换链纹理缺少 COPY_DST：钢琴卷帘/瀑布流的离屏合成拷贝会被 wgpu 校验拒绝（音符区黑屏）"
    );
    assert!(
        usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT),
        "交换链纹理必须可作为渲染目标，否则 UI 无法直接绘制到交换链"
    );
}
