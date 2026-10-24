//! 可见区间 cull（range cull）正确性契约。
//!
//! 契约：给定覆盖全部可见音符的候选区间，range cull 与整段 cull 必须
//! **逐位像素等价**；计划总数不一致时必须自动回退整段 cull（不得漏音）。

use crate::note_renderer::NoteRenderer;
use crate::note_renderer::types::CameraUniform;
use crate::{CullPlan, CullRange, NoteInstance};

const W: u32 = 256;
const H: u32 = 144;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.02,
    g: 0.02,
    b: 0.02,
    a: 1.0,
};
const NOTE_COLOR: [f32; 4] = [1.0, 0.2, 0.2, 1.0];

/// 相机：tick 400 在 x=0，tick 400+W 在 x=W（zoom=1, scroll=400）。
fn camera() -> CameraUniform {
    CameraUniform {
        scroll: [400.0, 0.0],
        zoom: [1.0, 20.0],
        viewport_size: [W as f32, H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [W as f32, H as f32],
        keyboard_width: 0.0,
        ruler_height: 0.0,
        max_key_index: 127.0,
        _padding: [0.0; 3],
    }
}

/// 1000 个按 start 升序的音符（tick 0..1000，长 10）。
fn scene() -> Vec<NoteInstance> {
    (0..1000u32)
        .map(|tick| NoteInstance::new(tick as f32, 60, 10.0, NOTE_COLOR, 1))
        .collect()
}

fn render(notes: &[NoteInstance], plan: Option<&CullPlan>) -> Vec<u8> {
    let (device, queue) = crate::test_gpu::shared_device();
    let mut renderer =
        NoteRenderer::new_onion_skin(&device, &queue, FORMAT, crate::GpuEpoch::INITIAL);
    renderer.set_view_state(&queue, 1, &[]);
    renderer.upload_instances(notes, &device, &queue);

    let size = wgpu::Extent3d {
        width: W,
        height: H,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("range_test_color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("range_test_depth"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: crate::constants::rendering::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("range_test_encoder"),
    });
    match plan {
        Some(plan) => renderer.prepare_pass_with_cull_plan(&mut encoder, camera(), &queue, plan),
        None => renderer.prepare_pass(&mut encoder, camera(), &queue),
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("range_test_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(CLEAR),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));
    read_pixels(&device, &queue, &color)
}

fn read_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = W * 4;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("range_test_staging"),
        size: (bytes_per_row * H) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("range_test_readback"),
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
            Ok(Err(e)) => panic!("回读 map 失败: {e:?}"),
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

#[test]
fn test_range_cull_matches_full_cull() {
    let notes = scene();
    // 可见 tick 窗口 [400, 656)，音符长 10 → 索引 390..=655 覆盖全部可见
    let plan = CullPlan {
        ranges: vec![CullRange {
            start: 390,
            count: 266,
        }],
        total_instances: notes.len(),
    };
    let full = render(&notes, None);
    let ranged = render(&notes, Some(&plan));
    assert_eq!(full, ranged, "单区间 range cull 必须与整段 cull 逐位等价");
}

#[test]
fn test_multiple_ranges_match_full_cull() {
    let notes = scene();
    // 同一可见窗口拆成两个相邻区间，验证 indirect 计数跨 dispatch 累加
    let plan = CullPlan {
        ranges: vec![
            CullRange {
                start: 390,
                count: 100,
            },
            CullRange {
                start: 490,
                count: 166,
            },
        ],
        total_instances: notes.len(),
    };
    assert_eq!(
        render(&notes, None),
        render(&notes, Some(&plan)),
        "多区间累加必须与整段 cull 逐位等价"
    );
}

#[test]
fn test_plan_total_mismatch_falls_back() {
    let notes = scene();
    let plan = CullPlan {
        ranges: vec![CullRange {
            start: 390,
            count: 266,
        }],
        // 故意错一个：渲染器必须回退整段 cull，而不是漏音
        total_instances: notes.len() - 1,
    };
    assert_eq!(
        render(&notes, None),
        render(&notes, Some(&plan)),
        "总数不一致必须回退整段 cull，结果与整段一致"
    );
}

#[test]
fn test_empty_plan_matches_full_cull() {
    let notes = scene();
    let plan = CullPlan {
        ranges: Vec::new(),
        total_instances: notes.len(),
    };
    assert_eq!(render(&notes, None), render(&notes, Some(&plan)));
}
