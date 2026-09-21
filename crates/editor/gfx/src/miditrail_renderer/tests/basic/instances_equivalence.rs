use super::*;

/// 等价性：`render_from_instances` 与 `render(手动换算)` 输出像素一致。
///
/// 换算逻辑从 `note_instances_to_miditrail` 逐行搬迁，覆盖边界：
/// 起始音符/跨视口长音符/右边界音符/越界 key（下游跳过）。
#[test]
fn test_render_from_instances_matches_manual_convert() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("测试需要可用的 wgpu 适配器");
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("miditrail_equiv_test_device"),
        required_features: adapter.features() & wgpu::Features::default(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .expect("请求 wgpu 设备失败");

    let uniform = MiditrailUniformGpu {
        tick: 480,
        ppq: 480,
        key_count: 128,
        frame_width: 320,
        frame_height: 180,
        kb_height: 20,
        _reserved: 0,
        speed: 1.0,
        param1: 0.0,
        param2: 0.0,
        fps: 60.0,
        z_far_distance: 7.5,
        view_mode: MiditrailViewMode::Normal,
        ticks_per_second: 960.0,
        _padding1: 0,
    };
    // (start, length, key)：起始/长音/右边界/越界 key 全覆盖。
    let raw = [
        (0u32, 1920u32, 60u8),
        (480, 480, 64),
        (960, 240, 200),
        (480, 960, 67),
    ];
    let instances: Vec<crate::NoteInstance> = raw
        .iter()
        .map(|&(s, l, k)| crate::NoteInstance {
            start_length: [s as f32, (l as f32).max(1.0)],
            key_color: crate::pack_key_color(k, [1.0, 0.0, 0.0, 1.0]),
            border_width: 0,
        })
        .collect();
    // 旧路径参考换算（与已删除的 `note_instances_to_miditrail` 同公式）。
    let manual: Vec<MiditrailNoteGpu> = instances
        .iter()
        .map(|n| {
            let (key, rgb) = crate::unpack_key_color(n.key_color);
            let start = n.start_length[0].max(0.0) as u32;
            let end = start.saturating_add(n.start_length[1].max(1.0) as u32);
            MiditrailNoteGpu {
                key: key as u32,
                start_tick: start,
                end_tick: end,
                color_packed: pack_color([rgb[0], rgb[1], rgb[2], 1.0]),
                track_idx: 0,
                velocity: 100,
                channel: 0,
                _padding: 0,
            }
        })
        .collect();

    let mut renderer_a = MiditrailRenderer::new(&device);
    let via_render =
        render_and_count_non_black(&device, &queue, &mut renderer_a, &uniform, &manual);

    // 独立 renderer 跑快捷路径（动画初始状态与 renderer_a 一致才可比）。
    // 同一 encoder 内 render_from_instances＋拷贝读回，一次 submit。
    let mut renderer_b = MiditrailRenderer::new(&device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("miditrail_equiv_shortcut"),
    });
    renderer_b.render_from_instances(&device, &queue, &mut encoder, &uniform, &instances);
    let texture = renderer_b.output_texture().expect("渲染后应存在输出纹理");
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = (320u32 * 4).next_multiple_of(align);
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("miditrail_equiv_staging"),
        contents: &vec![0u8; (padded * 180) as usize],
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
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
                bytes_per_row: Some(padded),
                rows_per_image: Some(180),
            },
        },
        wgpu::Extent3d {
            width: 320,
            height: 180,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).expect("map_async 回调发送失败");
    });
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    rx.recv()
        .expect("map_async 回调未收到")
        .expect("map_async 失败");
    let data = slice.get_mapped_range();
    let mut shortcut_non_black = 0usize;
    for row in 0..180 {
        let row_start = (row * padded) as usize;
        for col in 0..320 {
            let idx = row_start + (col * 4) as usize;
            if data[idx] != 0 || data[idx + 1] != 0 || data[idx + 2] != 0 {
                shortcut_non_black += 1;
            }
        }
    }
    drop(data);
    staging.unmap();

    assert!(via_render > 0, "参考路径应渲染出可见内容");
    assert_eq!(
        shortcut_non_black, via_render,
        "render_from_instances 输出像素应与手动换算一致"
    );
}
