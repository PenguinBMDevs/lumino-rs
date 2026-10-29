//! CPU 洋葱皮叠加层测试：纯逻辑断言 + GPU 读回像素证据。
//!
//! 脏矩形局部上传的验收在 `tests_partial.rs`；两者共用 `tests_support` 的
//! 离屏目标与读回路径。

use crate::RenderParams;
use crate::constants::rendering::{DEPTH_FORMAT, is_depth_stencil_compatible};
use crate::pipeline::test_device;

use super::{
    CpuOnionFrame, CpuOnionRenderer,
    pipeline::{self, CpuOnionUniform},
    tests_support::{TARGET_FORMAT, pixel, render_and_readback},
    upload::{UploadPlan, UploadState},
};

#[test]
fn cpu_onion_frame_defaults_to_none() {
    let params = RenderParams::default();
    assert!(
        params.cpu_onion_frame.is_none(),
        "默认必须为 None：不接入时渲染行为零变化"
    );
}

#[test]
fn cpu_onion_frame_builder_does_not_drift_from_default() {
    let built = RenderParams::builder().build();
    assert!(
        built.cpu_onion_frame.is_none(),
        "builder 默认值必须从主类型拷贝（不得漂移成 Some）"
    );

    let frame = CpuOnionFrame::new(1, 2, 3, 4, vec![0u8; 3 * 4 * 4], 11);
    let params = RenderParams::builder()
        .cpu_onion_frame(Some(frame.clone()))
        .build();
    let got = params
        .cpu_onion_frame
        .as_ref()
        .expect("builder 应把传入的 CPU 洋葱皮帧带进 RenderParams");
    assert_eq!(got.version, 11);
    assert_eq!(got.size(), (3, 4));

    // RenderParams 的 Clone / Debug 必须能处理 CpuOnionFrame（Debug 不打印像素）
    let cloned = params.clone();
    assert!(
        cloned.cpu_onion_frame.is_some(),
        "RenderParams::clone 必须保留 CPU 洋葱皮帧"
    );
    let text = format!("{cloned:?}");
    assert!(
        text.contains("cpu_onion_frame"),
        "RenderParams Debug 必须包含 CPU 洋葱皮字段"
    );
}

#[test]
fn cpu_onion_uniform_carries_physical_rect_and_viewport() {
    let frame = CpuOnionFrame::new(12, 34, 5, 6, vec![0u8; 5 * 6 * 4], 9);
    let uniform = CpuOnionUniform::new((800, 600), &frame);

    assert_eq!(uniform.viewport, [800.0, 600.0, 0.0, 0.0]);
    assert_eq!(
        uniform.rect,
        [12.0, 34.0, 5.0, 6.0],
        "uniform 必须按物理像素原样传递矩形（不做 DPI 缩放）"
    );
}

#[test]
fn cpu_onion_pipeline_depth_state_matches_ui_pass_contract() {
    let state = pipeline::depth_stencil_state();
    assert_eq!(state.format, DEPTH_FORMAT, "depth 格式必须与 UI pass 一致");
    assert!(!state.depth_write_enabled, "叠加层不得写深度");
    assert_eq!(state.depth_compare, wgpu::CompareFunction::Always);
    assert_eq!(state.bias.constant, 0);
    assert_eq!(state.bias.slope_scale, 0.0);
    assert_eq!(state.bias.clamp, 0.0);

    assert!(
        is_depth_stencil_compatible(true, Some(&state)),
        "带 depth attachment 的 UI pass 必须接受该管线"
    );
    assert!(
        !is_depth_stencil_compatible(false, Some(&state)),
        "无 depth 的导出 pass 不兼容——调用方必须先判定 depth 存在（本步不接导出）"
    );
}

#[test]
fn cpu_onion_upload_state_skips_repeated_version() {
    let mut state = UploadState::default();
    let frame = CpuOnionFrame::new(0, 0, 3, 3, vec![0u8; 3 * 3 * 4], 5);

    assert_eq!(state.plan(&frame), UploadPlan::RecreateAndUpload);
    state.record(&frame);
    assert_eq!(
        state.plan(&frame),
        UploadPlan::Reuse,
        "尺寸与版本都相同的帧不得重复上传"
    );
    assert_eq!(state.plan(&frame.clone()), UploadPlan::Reuse);
}

#[test]
fn cpu_onion_invalid_frame_is_not_drawn() {
    let (device, queue) = test_device();
    let mut renderer = CpuOnionRenderer::new(&device, &queue, TARGET_FORMAT);
    renderer.set_viewport(2, 2);

    let bad = CpuOnionFrame::new(0, 0, 2, 2, vec![0u8; 15], 1);
    renderer.prepare(&queue, &bad);
    assert_eq!(
        renderer.uploaded_version(),
        None,
        "像素长度不匹配的帧不得进入上传路径"
    );
}

/// GPU 读回：2x2 源帧（左上不透明红、其余透明）画到清黑目标上，验证：
/// - 不透明像素逐字节一致（sRGB 采样 → sRGB 写回的往返无损）；
/// - 透明像素保持清屏色（alpha = 0 不改变目标）。
#[test]
fn cpu_onion_gpu_quad_preserves_source_pixels() {
    let (device, queue) = test_device();

    let mut renderer = CpuOnionRenderer::new(&device, &queue, TARGET_FORMAT);
    renderer.set_viewport(2, 2);

    let mut pixels = vec![0u8; 16];
    pixels[0..4].copy_from_slice(&[255, 0, 0, 255]);
    let frame = CpuOnionFrame::new(0, 0, 2, 2, pixels, 1);
    renderer.prepare(&queue, &frame);
    assert_eq!(renderer.uploaded_version(), Some(1));

    // 同版本再次 prepare：内容未变化，不重复上传（版本保持 1）
    renderer.prepare(&queue, &frame);
    assert_eq!(
        renderer.uploaded_version(),
        Some(1),
        "相同 version 的帧不得触发重新上传"
    );

    let bytes = render_and_readback(&device, &queue, &mut renderer, &frame);
    assert_eq!(
        pixel(&bytes, 2, 0, 0),
        [255, 0, 0, 255],
        "不透明红像素必须逐字节一致（sRGB 采样/写回往返）"
    );
    for (x, y) in [(1, 0), (0, 1), (1, 1)] {
        assert_eq!(
            pixel(&bytes, 2, x, y),
            [0, 0, 0, 255],
            "透明像素必须保持清屏黑（alpha = 0 不改变目标）：({x}, {y})"
        );
    }
}
