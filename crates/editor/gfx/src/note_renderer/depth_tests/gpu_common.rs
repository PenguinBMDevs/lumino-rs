//! 深度像素证据 — GPU 测试共享装置（从 depth_tests.rs 拆出）

use super::*;

pub(super) const TEST_W: u32 = 256;
pub(super) const TEST_H: u32 = 144;
/// 连续渲染帧数（验收要求 ≥ 60 帧像素逐位一致）
pub(super) const ORDER_FRAMES: u32 = 64;

/// 深度 GPU 测试串行锁。
///
/// 本模块每个测试各自创建 wgpu 设备并做同步回读；并行执行时多设备争用会在
/// 驱动层放大（本机实测：并行时 GPU 工作卡死 → 测试进程异常退出）。模块内
/// 串行执行以保证稳定性；其余模块的并行度由各自测试决定。
static DEPTH_GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 获取深度 GPU 测试串行锁（忽略中毒：单个测试 panic 不应连带其他测试失败）。
pub(super) fn lock_depth_gpu() -> std::sync::MutexGuard<'static, ()> {
    DEPTH_GPU_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 测试用相机：x 方向 2 px/tick，y 方向 10 px/key，key 60 落在 y ∈ [0, 10)。
pub(super) fn test_camera() -> CameraUniform {
    CameraUniform {
        scroll: [0.0, 0.0],
        zoom: [2.0, 10.0],
        viewport_size: [TEST_W as f32, TEST_H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [TEST_W as f32, TEST_H as f32],
        keyboard_width: 0.0,
        ruler_height: 0.0,
        max_key_index: 60.0,
        _padding: [0.0; 3],
    }
}

/// 主音轨（track_enc = 1，也是 current_track）+ 两条洋葱皮轨道的重叠叠音场景。
///
/// 主音轨三音同色（真实场景：主轨统一蓝色）且互相重叠——修复前三者深度相同，
/// 谁在最前由可见缓冲顺序决定，描边随之逐帧闪烁；修复后由源索引稳定裁决
/// （索引小者在最前）：n1 的右边框应被 n0 填充覆盖。
pub(super) fn onion_scene() -> Vec<crate::NoteInstance> {
    // border_width 高 16 位 = track_enc，低 16 位 = 边框像素宽
    let encode = |track_enc: u32, border: u32| (track_enc << 16) | border;
    vec![
        crate::NoteInstance::new(0.0, 60, 100.0, [0.2, 0.55, 1.0, 1.0], encode(1, 2)),
        crate::NoteInstance::new(0.0, 60, 40.0, [0.2, 0.55, 1.0, 1.0], encode(1, 2)),
        crate::NoteInstance::new(20.0, 60, 60.0, [0.2, 0.55, 1.0, 1.0], encode(1, 2)),
        // 洋葱皮轨道内同样存在叠音：不同调色板色，顺序敏感
        crate::NoteInstance::new(0.0, 58, 100.0, [1.0, 0.2, 0.2, 1.0], encode(2, 2)),
        crate::NoteInstance::new(10.0, 58, 40.0, [0.2, 1.0, 0.2, 1.0], encode(2, 2)),
        crate::NoteInstance::new(0.0, 56, 100.0, [0.2, 0.2, 1.0, 1.0], encode(3, 2)),
    ]
}

/// 预览音符（哨兵 border_width）：与主音轨重叠，必须恒显示在最上层。
pub(super) fn preview_scene() -> Vec<crate::NoteInstance> {
    vec![crate::NoteInstance::new_preview(
        88.0,
        60,
        6.0,
        [1.0, 1.0, 1.0, 1.0],
    )]
}

/// 确定性伪随机置换：复刻 cull workgroup 抢占 `atomicAdd` 槽位的随机输出顺序。
/// 第 0 帧为原序（基线），后续每帧使用不同置换。
pub(super) fn permuted_order(len: usize, frame: u32) -> Vec<u32> {
    let mut order: Vec<u32> = (0..len as u32).collect();
    let mut state = 0x9E37_79B9_7F4A_7C15u64 ^ (u64::from(frame) + 1);
    for i in (1..order.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

/// 回读 RGBA8 纹理（256×144，行距 1024 天然满足 256B 对齐，无 padding）。
pub(super) fn readback_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Vec<u8> {
    let bytes_per_row = TEST_W * 4;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("note_depth_order_staging"),
        size: (bytes_per_row * TEST_H) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_depth_order_readback"),
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
                rows_per_image: Some(TEST_H),
            },
        },
        wgpu::Extent3d {
            width: TEST_W,
            height: TEST_H,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    // 非阻塞轮询 + 有界等待：禁止 `poll(Wait, timeout: None)`（无限阻塞——GPU 设备
    // 争用/驱动卡死时测试会永久挂起，deadline 检查永远轮不到）。`Poll` 只推进回调、
    // 不阻塞线程；超时以明确断言失败收尾，不挂死整个测试进程。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if let Ok(result) = rx.try_recv() {
            result.expect("深度顺序测试回读 map 失败");
            let data = slice.get_mapped_range();
            let out = data.to_vec();
            drop(data);
            staging.unmap();
            return out;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "深度顺序测试回读超时（30s，GPU 可能被驱动重置或设备争用卡死）"
        );
        let _ = device.poll(wgpu::PollType::Poll);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// 取像素（RGBA8）
pub(super) fn pixel(data: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * TEST_W + x) * 4) as usize;
    [
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]
}

/// 测试用清屏色（与音符颜色明显不同，便于做覆盖判定）
pub(super) const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.05,
    g: 0.05,
    b: 0.05,
    a: 1.0,
};

pub(super) fn make_color_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("note_depth_test_color"),
        size: wgpu::Extent3d {
            width: TEST_W,
            height: TEST_H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn make_depth_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("note_depth_test_depth"),
        size: wgpu::Extent3d {
            width: TEST_W,
            height: TEST_H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

pub(super) fn color_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(CLEAR_COLOR),
            store: wgpu::StoreOp::Store,
        },
        depth_slice: None,
    }
}

pub(super) fn depth_attachment(
    view: &wgpu::TextureView,
) -> wgpu::RenderPassDepthStencilAttachment<'_> {
    wgpu::RenderPassDepthStencilAttachment {
        view,
        depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Store,
        }),
        stencil_ops: None,
    }
}

/// 直接写相机 uniform（绕开 `prepare_pass` 的 cull，自行控制可见顺序）
pub(super) fn write_camera(renderer: &NoteRenderer, queue: &wgpu::Queue) {
    queue.write_buffer(
        renderer.viewport_buffer.inner(),
        0,
        bytemuck::cast_slice(&[test_camera()]),
    );
}

/// 直接写可见索引顺序 + 间接绘制参数（复刻 cull 的输出，用于控制/置换绘制顺序）
pub(super) fn write_visible_order(renderer: &NoteRenderer, queue: &wgpu::Queue, order: &[u32]) {
    queue.write_buffer(
        renderer.visible_instance_buffer.inner(),
        0,
        bytemuck::cast_slice(order),
    );
    let args = DrawIndirectArgs {
        vertex_count: 4,
        instance_count: order.len() as u32,
        first_vertex: 0,
        first_instance: 0,
        _padding: [0; 4],
    };
    queue.write_buffer(
        renderer.indirect_buffer.inner(),
        0,
        bytemuck::bytes_of(&args),
    );
}

/// 覆盖掩码：像素是否被音符绘制（与清屏色不同）。
///
/// 背景参考取画面右下角——测试场景的音符全部落在 x < 200（tick ≤ 100）与
/// y < 50（key ≥ 56）之内，该点必然只有清屏色；同时避开 sRGB 目标格式下
/// 清屏色编码后的精确取值问题。
pub(super) fn coverage_mask(pixels: &[u8]) -> Vec<bool> {
    let background = background_pixel(pixels);
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|px| px != &background)
        .collect()
}

/// 背景像素（画面右下角，场景内无音符覆盖）
pub(super) fn background_pixel(pixels: &[u8]) -> [u8; 4] {
    pixel(pixels, TEST_W - 1, TEST_H - 1)
}

/// 适配器深度分辨率探测（能力门控，机制无关）：
///
/// 区域化位空间深度（主音轨区 ≈ 2^-126 / 洋葱皮区 ≈ 2^-67，相差 ~2^59 倍）
/// 依赖适配器以 **float32 语义**比较深度。软件光栅器（例如 CI ubuntu 的 Mesa
/// 软渲染）在 [0,1]→NDC 变换或定点深度实现下会把亚 2^-24 的深度全部压平，
/// 此时「绘制顺序无关 / 深度差裁决」类断言在该适配器上不可判定。
///
/// 探测场景：同几何的主音轨音符（先画；shader 强制主轨蓝，深度更小）与洋葱皮
/// 音符（后画；红，深度更大）。float32 语义下后者必须被 `LessEqual` 拒绝
/// （重叠区保持主轨蓝）；压平或深度失效时后画者胜（重叠区变红）。
///
/// 该探测只判能力、不改语义：管线/bind group 若有真实错误会以 wgpu 校验 panic
/// 暴露，不会被静默吞掉；返回 `false` 时调用方按明确原因跳过细粒度深度断言。
pub(super) fn adapter_resolves_region_depth(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> bool {
    // 进程内缓存：同一测试进程共享同一适配器，探针只需渲染一次（减少 GPU 同步点）
    static PROBE_RESULT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PROBE_RESULT.get_or_init(|| probe_region_depth(device, queue, format))
}

/// 探针实现（见 [`adapter_resolves_region_depth`] 说明）。
pub(super) fn probe_region_depth(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> bool {
    // border_width 高 16 位：1 = 主音轨；2 = 洋葱皮（当前轨 = 1）
    let encode = |track_enc: u32| (track_enc << 16) | 2;
    let notes = vec![
        // 主音轨音符：实例色被 shader 覆盖为主轨蓝
        crate::NoteInstance::new(0.0, 60, 100.0, [0.0, 0.0, 0.0, 1.0], encode(1)),
        // 洋葱皮音符：纯红
        crate::NoteInstance::new(0.0, 60, 100.0, [1.0, 0.0, 0.0, 1.0], encode(2)),
    ];

    let mut onion = NoteRenderer::new_onion_skin(device, queue, format);
    onion.set_view_state(queue, 1, &[]);
    onion.upload_instances(&notes, device, queue);
    write_camera(&onion, queue);
    // 手工可见顺序：主音轨先画、洋葱皮后画（绕开 cull 的随机输出顺序）
    write_visible_order(&onion, queue, &[0, 1]);

    let color_texture = make_color_texture(device, format);
    let depth_texture = make_depth_texture(device);
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_depth_capability_probe"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_depth_capability_probe"),
            color_attachments: &[Some(color_attachment(&color_view))],
            depth_stencil_attachment: Some(depth_attachment(&depth_view)),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        onion.draw(&mut pass, true, None);
    }
    queue.submit(Some(encoder.finish()));
    let pixels = readback_pixels(device, queue, &color_texture);

    // 填充区中心（x=50, y=5）：主轨蓝 (124,196,255) vs 洋葱皮红 (255,0,0)
    let px = pixel(&pixels, 50, 5);
    let main_won = px[2] > px[0] && px[1] > 100;
    let onion_won = px[0] > px[2] && px[1] < 100;
    let resolved = main_won && !onion_won;
    if !resolved {
        eprintln!(
            "[depth-capability] 适配器深度压平/失效：探针像素 {px:?}（期望主轨蓝胜出）——\
             细粒度深度断言在本适配器上不可判定"
        );
    }
    resolved
}
