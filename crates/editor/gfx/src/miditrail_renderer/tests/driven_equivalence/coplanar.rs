use super::*;

// ───────────────────────── 共面叠音闪烁回归（RCA 锁） ─────────────────────────
//
// 根因：driven 曾用真实深度排序（`depth_write=true`）。同键叠音顶面完全共面，
// 深度只差 ULP：随帧滑动逐像素 winner 在两种颜色间翻转 → 真机视频"疯狂闪烁"
//（修复前本测试实测 946 像素红↔绿翻转；legacy 0）。legacy 画家排序注释即为
// 此设计（见 `instances.rs::build_note_instances`）。
//
// 修复后（compact 画家序 + `depth_write=false`，绘制顺序即最终次序）：同键叠音
// 按稳定序绘制，连续两帧红↔绿翻转必须为 0。
#[test]
fn test_coplanar_overlap_no_flicker() {
    let (_instance, device, queue) = test_device();
    let mut uniform = test_uniform();
    let (w, h) = (uniform.frame_width, uniform.frame_height);
    let frames = [5000u32, 5016];

    let scene = || {
        vec![
            NoteInstance::new(4000.0, 60, 8000.0, [1.0, 0.0, 0.0, 1.0], 0),
            NoteInstance::new(4000.0, 60, 4800.0, [0.0, 1.0, 0.0, 1.0], 0),
        ]
    };
    // 红↔绿主导翻转计数（阈值 32 抗提亮与量化噪声）。
    let flip_count = |a: &[u8], b: &[u8]| -> usize {
        a.as_chunks::<4>()
            .0
            .iter()
            .zip(b.as_chunks::<4>().0.iter())
            .filter(|(pa, pb)| {
                let sa = pa[0] as i32 - pa[1] as i32;
                let sb = pb[0] as i32 - pb[1] as i32;
                (sa > 32 && sb < -32) || (sa < -32 && sb > 32)
            })
            .count()
    };

    // A：driven（生产默认平面模式）。
    let mut driven_frames: Vec<Vec<u8>> = Vec::new();
    let mut driven = MiditrailRenderer::new(&device);
    driven.flat_notes = true;
    for &tick in &frames {
        uniform.tick = tick;
        let notes = scene();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("flicker_probe_driven"),
        });
        driven.render_gpu_driven(&device, &queue, &mut encoder, &uniform, &notes);
        queue.submit(std::iter::once(encoder.finish()));
        let tex = driven.output_texture().expect("driven 应有输出");
        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("flicker_probe_driven_rb"),
        });
        driven_frames.push(readback_pixels(&device, &queue, encoder, tex, w, h));
    }

    // B：legacy 画家路径（对照组，理论只差边缘移动像素）。
    let mut legacy_frames: Vec<Vec<u8>> = Vec::new();
    let mut legacy = MiditrailRenderer::new(&device);
    legacy.flat_notes = true;
    for &tick in &frames {
        uniform.tick = tick;
        let notes = scene();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("flicker_probe_legacy"),
        });
        legacy.render_from_instances(&device, &queue, &mut encoder, &uniform, &notes);
        queue.submit(std::iter::once(encoder.finish()));
        let tex = legacy.output_texture().expect("legacy 应有输出");
        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("flicker_probe_legacy_rb"),
        });
        legacy_frames.push(readback_pixels(&device, &queue, encoder, tex, w, h));
    }

    let driven_flip = flip_count(&driven_frames[0], &driven_frames[1]);
    let legacy_flip = flip_count(&legacy_frames[0], &legacy_frames[1]);
    assert_eq!(legacy_flip, 0, "legacy 画家路径基准不得翻转");
    assert_eq!(
        driven_flip, 0,
        "同键共面叠音不得跨帧翻转（真实深度 ULP 闪烁回归；修复前实测 946）"
    );
}
