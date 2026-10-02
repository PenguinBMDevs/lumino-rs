//! VS cull 直绘路径的正确性（PREF-005）
//!
//! 本卡把 UI 路径的 **compute cull** 换成「`vs_direct` 内自判可见 + 按
//! `instance_index` 升序直绘」。这同时改了两件事：
//!
//! 1. **绘制顺序**：cull 路径的可见索引由 workgroup 间抢占式 `atomicAdd` 写入
//!    （帧间不稳定），直绘路径是源索引升序（= `region_depth` 近→远）；
//! 2. **混合状态**：洋葱皮管线由 alpha 混合改为替换写入（FS 恒不透明）。
//!
//! 两者都不允许改变画面——REND-001 的 `tie_break_depth`（深度与绘制顺序无关）
//! 是前提。因此本模块的第一条契约是**逐位像素等价**，这是「改绘制序零视觉变化」
//! 唯一可信的验证方式（比"看起来一样"强，也比只看某几个像素强）。
//!
//! 第二条契约是 `vs_direct` 的自判可见必须与 `cull.wgsl` 同口径：视口外 /
//! 长度 ≤ 0 / 静音轨一律不得产生片元。

use crate::NoteInstance;
use crate::note_renderer::NoteRenderer;
use crate::note_renderer::types::CameraUniform;

/// 预览层必须创建 VS 直绘管线（z-order 闪烁修复，2026-10）
///
/// 契约：预览实例深度恒为 0.0（`note.wgsl` 哨兵分支，恒覆盖文档音符），
/// 重叠矩形的赢家只能由**绘制顺序**决定 —— 「后画者胜」。若预览层退回
/// cull 路径，可见槽位由抢占式 `atomicAdd` 分配、顺序逐帧随机，赢家就逐帧
/// 翻转（闪烁）。所以「预览层存在直绘管线」（`instance_index` 升序 = 提交序）
/// 是**后来者居上**的必要条件，本测试把它钉死，防止后续为省启动编译时间
/// 再次收窄创建范围而静默回退（原 `is_onion && needs_depth` 门控即此坑）。
#[test]
fn test_preview_layer_has_direct_pipeline() {
    let (device, queue) = crate::test_gpu::shared_device();
    let preview = NoteRenderer::new(&device, &queue, FORMAT);
    assert!(
        preview.direct_pipeline.is_some(),
        "预览层必须创建 VS 直绘管线：否则预览实例走 cull 随机序，重叠区 z-order 闪烁"
    );
    // 导出无 depth 变体不建（`draw_direct` 内部自动回退 cull 路径）
    let export = NoteRenderer::new_without_depth(&device, &queue, FORMAT);
    assert!(
        export.direct_pipeline.is_none(),
        "无 depth 变体（导出）不应创建直绘管线"
    );
    assert!(
        preview.vertical_direct_pipeline.is_some(),
        "预览层必须创建纵向直绘管线：否则纵向卷帘下重叠预览矩形仍按 cull 随机序闪烁"
    );
    // 洋葱皮只建横向直绘（纵向仍走 cull 路径），不为它多付一次大 shader 管线编译
    let onion = NoteRenderer::new_onion_skin(&device, &queue, FORMAT);
    assert!(
        onion.direct_pipeline.is_some() && onion.vertical_direct_pipeline.is_none(),
        "洋葱皮应只有横向直绘管线（纵向仍走 cull 路径）"
    );
    assert!(
        export.vertical_direct_pipeline.is_none(),
        "无 depth 变体（导出）不应创建纵向直绘管线"
    );
}

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

/// 轨道编码（高 16 位）+ 描边像素宽（低 16 位）
fn encode(track_enc: u32, border: u32) -> u32 {
    (track_enc << 16) | border
}

fn camera(zoom: [f32; 2], max_key_index: f32) -> CameraUniform {
    CameraUniform {
        scroll: [0.0, 0.0],
        zoom,
        viewport_size: [W as f32, H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [W as f32, H as f32],
        keyboard_width: 0.0,
        ruler_height: 0.0,
        max_key_index,
        _padding: [0.0; 3],
    }
}

/// 重叠加叠场景：主音轨三音同色互相重叠（绘制序敏感的经典构造）+
/// 两条洋葱皮轨道（其中一条静音）。
fn overlap_scene() -> Vec<NoteInstance> {
    vec![
        // 主音轨（track_enc 1 = current_track）：三音同 key 同色互相重叠
        NoteInstance::new(0.0, 120, 60.0, NOTE_COLOR, encode(1, 2)),
        NoteInstance::new(0.0, 120, 30.0, NOTE_COLOR, encode(1, 2)),
        NoteInstance::new(10.0, 120, 40.0, NOTE_COLOR, encode(1, 2)),
        // 洋葱皮轨道（不同调色板色，同样存在叠音）
        NoteInstance::new(0.0, 118, 60.0, [0.2, 1.0, 0.2, 1.0], encode(2, 2)),
        NoteInstance::new(10.0, 118, 40.0, [0.2, 0.2, 1.0, 1.0], encode(2, 2)),
        // 静音轨（track_idx 2 → track_enc 3）：两条路径都必须剔除
        NoteInstance::new(0.0, 116, 60.0, [1.0, 1.0, 1.0, 1.0], encode(3, 2)),
    ]
}

/// 用指定路径渲染一帧并回读像素。
///
/// `direct = true` 走 `prepare_direct` + `draw_direct`（VS cull）；
/// `false` 走 `prepare_pass` + `draw`（compute cull + 可见索引）。
fn render(notes: &[NoteInstance], camera: CameraUniform, direct: bool) -> Vec<u8> {
    let (device, queue) = crate::test_gpu::shared_device();

    let mut renderer = NoteRenderer::new_onion_skin(&device, &queue, FORMAT);
    // current_track = 1（track_enc 1）；track_idx 2（track_enc 3）静音
    renderer.set_view_state(&queue, 1, &[2]);
    renderer.upload_instances(notes, &device, &queue);

    let size = wgpu::Extent3d {
        width: W,
        height: H,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("direct_test_color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("direct_test_depth"),
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
        label: Some("direct_test_encoder"),
    });
    if direct {
        renderer.prepare_direct(camera, &queue);
    } else {
        renderer.prepare_pass(&mut encoder, camera, &queue);
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("direct_test_pass"),
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
        if direct {
            renderer.draw_direct(&mut pass, true, None);
        } else {
            renderer.draw(&mut pass, true, None);
        }
    }
    queue.submit(Some(encoder.finish()));
    read_pixels(&device, &queue, &color)
}

/// 回读整张 RGBA8 纹理（非阻塞轮询 + 有界等待）。
fn read_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = W * 4;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("direct_test_staging"),
        size: (bytes_per_row * H) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("direct_test_readback"),
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

fn covered_pixels(pixels: &[u8]) -> usize {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] > 8 || px[1] > 8 || px[2] > 8)
        .count()
}

/// **契约 1（本卡核心）**：VS cull 直绘与 compute cull 路径**逐位像素一致**。
///
/// 覆盖两种缩放档位：常规缩放（quad 有面积）与亚像素全景（quad 亚像素宽）。
/// 亚像素档位下两者都可能只覆盖很少像素——因此额外断言常规档位必须有覆盖，
/// 防止「两边都画空」这种假绿。
#[test]
fn test_direct_path_pixels_match_cull_path() {
    let notes = overlap_scene();
    let cases = [
        ("常规缩放", camera([2.0, 10.0], 127.0)),
        (
            "亚像素全景",
            camera([W as f32 / 1_000_000.0, H as f32 / 128.0], 127.0),
        ),
    ];

    for (label, cam) in cases {
        let cull = render(&notes, cam, false);
        let direct = render(&notes, cam, true);
        assert_eq!(
            covered_pixels(&cull),
            covered_pixels(&direct),
            "{label}：两条路径覆盖像素数不一致"
        );
        assert_eq!(
            cull, direct,
            "{label}：VS cull 直绘与 compute cull 路径像素不一致——\
             说明改绘制序/改混合产生了视觉差异（REND-001 前提被破坏）"
        );
    }

    // 反假绿守卫：常规档位必须真的画出东西
    let normal = render(&notes, camera([2.0, 10.0], 127.0), true);
    assert!(
        covered_pixels(&normal) > 0,
        "直绘路径在常规缩放下必须产生片元（否则上面的相等断言是空对空）"
    );
}

/// **契约 2**：`vs_direct` 自判可见与 `cull.wgsl` 同口径——视口外、长度 ≤ 0、
/// 静音轨一律不产生片元。
#[test]
fn test_direct_path_self_culls_invisible_notes() {
    let notes = vec![
        // key 越界（常规相机可见 key 约 113..127）
        NoteInstance::new(0.0, 5, 100.0, NOTE_COLOR, encode(1, 2)),
        // tick 远越界
        NoteInstance::new(1.0e7, 120, 100.0, NOTE_COLOR, encode(1, 2)),
        // 非法长度
        NoteInstance::new(0.0, 120, 0.0, NOTE_COLOR, encode(1, 2)),
        // 静音轨（track_idx 2）
        NoteInstance::new(0.0, 120, 100.0, NOTE_COLOR, encode(3, 2)),
    ];

    let pixels = render(&notes, camera([2.0, 10.0], 127.0), true);
    assert_eq!(
        covered_pixels(&pixels),
        0,
        "视口外 / 长度 ≤ 0 / 静音轨的音符在直绘路径下不得产生任何片元"
    );
}
