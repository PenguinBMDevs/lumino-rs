//! 视频导出（无 depth 变体）绘制前的 cull 契约探针 — P0 先决缺陷取证
//!
//! # 为什么需要这组测试
//!
//! `render_pass.rs` 的 `else` 分支对**音符主层**调用的是
//! `note.prepare_direct(camera, queue)`（只写相机 uniform，**不 dispatch cull**），
//! 随后调用 `note.draw_direct(...)`。而导出渲染器按 `needs_depth=false` 创建
//! （`Renderers::new_for_video_export`），`direct_pipeline` 恒为 `None`，
//! 于是 `draw_direct` 回退到 `draw_with_pipeline` → `draw_indirect(indirect_buffer)`。
//!
//! `indirect_buffer` 只在 `prepare_pass_with_pipeline` 里被写入（模板重置 + cull
//! 原子递增）。导出路径从不调用 `prepare_pass` ⇒ 参数恒为创建时的全零 ⇒
//! `vertex_count = 0`、`instance_count = 0` ⇒ **一个音符都不画**。
//!
//! 这组测试把上述契约钉死：
//! 1. `prepare_direct` 之后 indirect 参数必须仍为空（导出不得依赖它）；
//! 2. `prepare_pass` 之后 indirect 参数必须带可见实例数（导出必须走这条）。
//!
//! 若第 1 条将来变红（`prepare_direct` 开始跑 cull），说明上游分支已改，
//! 应同步复核 `render_pass.rs` 的分支选择，而不是删掉本测试。

use super::NoteRenderer;
use crate::note_renderer::types::CameraUniform;
use crate::{GpuEpoch, NoteInstance};

const W: u32 = 256;
const H: u32 = 144;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// 轨道编码（高 16 位）+ 描边像素宽（低 16 位）
fn encode(track_enc: u32, border: u32) -> u32 {
    (track_enc << 16) | border
}

/// 与 `direct_tests` 同口径的相机（notes 落在视口内）。
fn camera() -> CameraUniform {
    CameraUniform {
        scroll: [0.0, 0.0],
        zoom: [2.0, 10.0],
        viewport_size: [W as f32, H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [W as f32, H as f32],
        keyboard_width: 0.0,
        ruler_height: 0.0,
        max_key_index: 127.0,
        _padding: [0.0; 3],
    }
}

/// 两条必然落在视口内的音符。
fn visible_notes() -> Vec<NoteInstance> {
    vec![
        NoteInstance::new(0.0, 120, 60.0, [1.0, 0.2, 0.2, 1.0], encode(1, 2)),
        NoteInstance::new(10.0, 118, 40.0, [0.2, 1.0, 0.2, 1.0], encode(1, 2)),
    ]
}

/// 创建导出变体渲染器（无 depth、无直绘管线）并上传可见音符。
fn export_renderer_with_notes() -> (NoteRenderer, wgpu::Device, wgpu::Queue) {
    let (device, queue) = crate::test_gpu::shared_device();
    let mut renderer = NoteRenderer::new_without_depth(&device, &queue, FORMAT, GpuEpoch::INITIAL);
    renderer.set_view_state(&queue, 1, &[]);
    renderer.upload_instances(&visible_notes(), &device, &queue);
    (renderer, device, queue)
}

/// 读回 indirect 参数前 32 字节（`DrawIndirectArgs` = 8 × u32）。
///
/// 布局（与 `shaders/cull.wgsl::DrawIndirectArgs` 一致）：
/// `vertex_count | instance_count | first_vertex | first_instance | padding × 4`
fn read_indirect_args(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &NoteRenderer,
) -> [u32; 8] {
    const SIZE: u64 = 32;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("export_probe_indirect_staging"),
        size: SIZE,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("export_probe_indirect_copy"),
    });
    encoder.copy_buffer_to_buffer(renderer.indirect_buffer.inner(), 0, &staging, 0, SIZE);
    queue.submit(Some(encoder.finish()));

    let bytes = crate::readback_bytes_sync(device, &staging, SIZE as usize)
        .expect("读回 indirect 参数失败");
    let mut args = [0u32; 8];
    for (slot, chunk) in args.iter_mut().zip(bytes.as_chunks::<4>().0.iter()) {
        *slot = u32::from_le_bytes(*chunk);
    }
    args
}

/// **契约 1（缺陷取证）**：导出路径实际调用的 `prepare_direct` 不 dispatch cull，
/// 因此 indirect 参数保持全零 —— 此时 `draw_indirect` 不会产生任何图元。
///
/// 这条断言存在的意义：它是「导出主音符层必须改调用 `prepare_pass`」的机器证据。
#[test]
fn prepare_direct_leaves_indirect_args_empty() {
    let (renderer, device, queue) = export_renderer_with_notes();

    // 复刻导出路径（render_pass.rs 的 else 分支）：只 prepare_direct，不跑 cull。
    renderer.prepare_direct(camera(), &queue);

    let args = read_indirect_args(&device, &queue, &renderer);
    assert_eq!(
        args[0], 0,
        "prepare_direct 不重置 indirect 模板，vertex_count 应为创建时的 0（实测 {}）",
        args[0]
    );
    assert_eq!(
        args[1], 0,
        "prepare_direct 不 dispatch cull，instance_count 应为 0（实测 {}）；\
         若此处变红说明 prepare_direct 开始跑 cull，需同步复核 render_pass.rs 分支",
        args[1]
    );
}

/// **契约 2（正确路径）**：`prepare_pass` 跑完 cull 后，indirect 参数必须带上
/// 「4 顶点 + 可见实例数」。这是导出主音符层唯一合法的准备入口。
#[test]
fn prepare_pass_populates_indirect_args() {
    let (mut renderer, device, queue) = export_renderer_with_notes();

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("export_probe_cull"),
    });
    renderer.prepare_pass(&mut encoder, camera(), &queue);
    queue.submit(Some(encoder.finish()));

    let args = read_indirect_args(&device, &queue, &renderer);
    assert_eq!(args[0], 4, "quad 路径每实例 4 顶点");
    assert_eq!(
        args[1],
        visible_notes().len() as u32,
        "cull 后可见实例数应等于两条眼内音符（实测 顶点 {} / 实例 {}）",
        args[0],
        args[1]
    );
}

/// 用指定准备方式渲染一帧，返回覆盖像素数与像素缓冲。
///
/// `cull = true` 复刻修复后的导出路径（`prepare_pass` + cull）；
/// `false` 复刻修复前的导出路径（`prepare_direct`，不 dispatch cull）。
fn render_export_frame(cull: bool) -> (usize, Vec<u8>) {
    let (mut renderer, device, queue) = export_renderer_with_notes();

    let size = wgpu::Extent3d {
        width: W,
        height: H,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("export_probe_color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("export_probe_frame"),
    });
    if cull {
        renderer.prepare_pass(&mut encoder, camera(), &queue);
    } else {
        renderer.prepare_direct(camera(), &queue);
    }
    {
        // 与 `execute_render_pass` 的导出分支同构：无 depth attachment。
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("export_probe_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    // 纯黑：任何音符像素都会显著偏离它
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw_direct(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));

    let pixels = read_pixels(&device, &queue, &color);
    let covered = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] > 8 || px[1] > 8 || px[2] > 8)
        .count();
    (covered, pixels)
}

/// 回读整张 RGBA8 纹理（非阻塞轮询 + 有界等待）。
fn read_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = W * 4;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("export_probe_readback"),
        size: (bytes_per_row * H) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("export_probe_readback_copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(H),
            },
        },
        wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let _ = device.poll(wgpu::PollType::Poll);
        match rx.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(error)) => panic!("回读 map 失败: {error:?}"),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                assert!(std::time::Instant::now() < deadline, "回读 30s 未就绪");
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => panic!("回读回调丢失"),
        }
    }
    let data = slice.get_mapped_range().to_vec();
    staging.unmap();
    data
}

/// **契约 4（缺陷的可观测形态）**：按修复前的导出路径（`prepare_direct` + 无 depth）
/// 渲染，**覆盖像素数为 0** —— 画面上一个音符都没有。
///
/// 这条断言把「间接绘制参数未写入」翻译成用户能感知的现象（空帧），
/// 避免只对着 buffer 里的一堆 0 讨论。
#[test]
fn broken_export_path_draws_no_notes() {
    let (covered, _pixels) = render_export_frame(false);
    assert_eq!(
        covered, 0,
        "修复前的导出路径（prepare_direct 不跑 cull）应当一个音符都画不出来；\
         若此处变红，说明渲染分支已修，应删除本用例或改为正向断言"
    );
}

/// **契约 5（修复后的正确形态）**：走 `prepare_pass`（cull）后，音符必须真的出现，
/// 且同场景连渲两次**逐字节相同**（无 depth 时靠绘制序稳定；有 depth 时靠全序）。
#[test]
fn fixed_export_path_draws_notes_deterministically() {
    let (covered_a, pixels_a) = render_export_frame(true);
    assert!(
        covered_a > 0,
        "修复后的导出路径必须产生音符片元（否则上面的「修复前为 0」是空对空）"
    );

    let (covered_b, pixels_b) = render_export_frame(true);
    assert_eq!(covered_a, covered_b, "两次渲染的覆盖像素数必须一致");
    assert_eq!(
        pixels_a, pixels_b,
        "同一场景两次渲染必须逐字节相同：导出帧不确定 ⇒ 像素级一致无从验收"
    );
}

///
/// 注意：本测试只校验「集合稳定」，不校验「顺序稳定」——顺序由 cull 的
/// 抢占式 `atomicAdd` 决定，只有配合 depth attachment（`region_depth` 全序）
/// 才能让重叠区赢家与顺序无关。顺序无关性由 `depth_tests` 覆盖。
/// **契约 3（确定性前提）**：同一场景连续跑 cull 两次，可见实例集必须稳定。
#[test]
fn prepare_pass_is_stable_across_repeats() {
    let (mut renderer, device, queue) = export_renderer_with_notes();

    let mut previous: Option<[u32; 8]> = None;
    for round in 0..2 {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("export_probe_cull_repeat"),
        });
        renderer.prepare_pass(&mut encoder, camera(), &queue);
        queue.submit(Some(encoder.finish()));

        let args = read_indirect_args(&device, &queue, &renderer);
        // 只比较 vertex/instance 计数（padding 槽位不具语义）。
        let counts = [args[0], args[1], args[2], args[3]];
        if let Some(prev) = previous {
            assert_eq!(
                counts,
                [prev[0], prev[1], prev[2], prev[3]],
                "第 {round} 轮 cull 的间接参数与上一轮不一致：可见实例集不稳定"
            );
        }
        previous = Some(args);
    }
}
