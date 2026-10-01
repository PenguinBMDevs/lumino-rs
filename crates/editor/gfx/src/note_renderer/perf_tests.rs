//! 音符渲染 GPU 耗时分解基准（PREF-004 P1 决策依据）
//!
//! # 为什么需要它
//!
//! PREF-004 卡片关于「每帧 GPU 成本」的结论（`cull ∝ 总音符数`、`fill ∝ 可见音符数`、
//! 「1.5 亿 ≈ 18.4ms」）全部来自带宽 / 像素率的**估算**，没有任何真机分段数字。
//! 而候选优化各自对应**不同**瓶颈，选错方向等于白干：
//!
//! | 候选 | 砍掉的是 | 只有在……时才有效 |
//! |---|---|---|
//! | 全景直绘（跳过 cull） | cull 的读 + 可见索引写 | cull 段占 `T` 显著比例 |
//! | 图元量削减（点图元 / 密度层） | 顶点与光栅化 | render 段由**图元吞吐**主导 |
//! | 去掉 VS 二次读 | 顶点阶段的 storage 读 | render 段由**带宽**主导 |
//!
//! 本基准把每帧 GPU 成本拆成 **clear / cull pass / render pass** 三段可测数字。
//!
//! # 测量方法与局限（必须诚实标注）
//!
//! 每段单独 encode → `queue.submit` → 有界 `poll(Wait)` 到完成，取墙钟中位数
//! （首轮丢弃为预热）。因此测的是**串行化**的分段耗时（段间无重叠），绝对值比
//! 真实帧偏悲观；本基准的用途是**段间比例**与**随实例数的增长趋势**，不是绝对帧时。
//!
//! # 运行
//!
//! ```text
//! cargo test -p lumino-gfx --lib perf_tests -- --ignored --nocapture --test-threads=1
//! LUMINO_PERF_NOTES=2000000,8000000,16000000 ITERS=7 cargo test ...
//! ```
//!
//! 默认实例档位取共享测试设备允许的规模；`LUMINO_PERF_NOTES` 可覆盖。
//! 输出含适配器信息——**卡片验收要求的「硬件环境」由此固化**。

use std::time::{Duration, Instant};

use super::NoteRenderer;
use super::types::CameraUniform;
use crate::NoteInstance;

/// 基准视口（与真机 1080p 走查口径对齐）
const BENCH_W: u32 = 1920;
const BENCH_H: u32 = 1080;
/// 键盘区宽度 / 标尺高度（与编辑器默认量级一致）
const KEYBOARD_W: f32 = 60.0;
const RULER_H: f32 = 30.0;
/// 琴键数（128 键标准）
const KEY_COUNT: u32 = 128;
/// 全曲 tick 数：ppq 480 × 4/4 × 200 小节（长黑乐谱量级）
const TOTAL_TICKS: f32 = 480.0 * 4.0 * 200.0;
/// 音符长度：16 分音符（120 tick）
const NOTE_LEN: f32 = 120.0;

/// 默认实例档位（覆盖「百万级普通 MIDI」到「千万级黑乐谱」）
const DEFAULT_COUNTS: &[usize] = &[2_000_000, 8_000_000, 16_000_000];
/// 默认重复轮数（首轮丢弃为预热）
const DEFAULT_ITERS: usize = 7;

/// 基准用 GPU 上下文。
struct BenchGpu {
    #[allow(dead_code)]
    instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    adapter_info: String,
    /// 单次 storage binding 上限（打印用，解释分块行为）
    binding_limit: u32,
    /// 单 buffer 上限
    buffer_limit: u64,
}

impl BenchGpu {
    /// 创建基准专用设备：把 `max_buffer_size` / `max_storage_buffer_binding_size`
    /// 放到适配器真实上限（共享测试设备用 `Limits::default()` 会被 256MB 卡住，
    /// 无法覆盖黑乐谱规模）。其余 limits 保持默认值以免请求失败。
    fn new() -> Self {
        use futures::executor::block_on;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("基准需要可用适配器");
        let info = adapter.get_info();
        let adapter_limits = adapter.limits();

        let limits = wgpu::Limits {
            max_buffer_size: adapter_limits.max_buffer_size,
            max_storage_buffer_binding_size: adapter_limits.max_storage_buffer_binding_size,
            ..wgpu::Limits::default()
        };

        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("lumino_gfx_perf_bench_device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }))
        .expect("基准设备创建失败");

        eprintln!(
            "[perf] adapter={} backend={:?} type={:?} driver={} {}",
            info.name, info.backend, info.device_type, info.driver, info.driver_info
        );
        eprintln!(
            "[perf] limits: buffer={}MB binding={}MB",
            adapter_limits.max_buffer_size / 1024 / 1024,
            adapter_limits.max_storage_buffer_binding_size / 1024 / 1024
        );

        Self {
            instance,
            device,
            queue,
            // 时序与色彩空间无关，取最通用的 8bit 可渲染格式
            format: wgpu::TextureFormat::Rgba8Unorm,
            adapter_info: format!("{} ({:?})", info.name, info.backend),
            binding_limit: adapter_limits.max_storage_buffer_binding_size,
            buffer_limit: adapter_limits.max_buffer_size,
        }
    }

    /// 该设备能安全承载的实例数上限（留 25% 余量给可见索引缓冲与其它资源）
    fn max_instances(&self, visible_buffer_bytes_per_instance: u64) -> usize {
        let per_instance =
            std::mem::size_of::<NoteInstance>() as u64 + visible_buffer_bytes_per_instance;
        let chunk_cap = u64::from(self.binding_limit);
        let budget = self.buffer_limit.min(chunk_cap).saturating_mul(3) / 4;
        (budget / per_instance) as usize
    }
}

/// 合成黑乐谱风格音符：tick 均匀铺满全曲、key 落在 21..108（88 键区间），
/// 制造「全景时几乎全部可见 + 大量亚像素 quad 堆叠」的最坏负载。
fn synth_notes(count: usize) -> Vec<NoteInstance> {
    const KEY_LOW: u32 = 21;
    const KEY_SPAN: u32 = 88;

    let mut notes = Vec::with_capacity(count);
    // 确定性伪随机（xorshift）：避免依赖 rand，且跨机器可复现
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..count {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let tick = (state % TOTAL_TICKS as u64) as f32;
        let key = KEY_LOW + (state >> 32) as u32 % KEY_SPAN;
        // border_width 高 16 位 = 轨道编码（1 = 主音轨），低 16 位 = 描边像素宽
        notes.push(NoteInstance::new(
            tick,
            key as u8,
            NOTE_LEN,
            [0.2, 0.55, 1.0, 1.0],
            (1u32 << 16) | 1,
        ));
    }
    notes
}

/// 全景相机：全曲 tick + 全 128 键可见 ⇒ 所有音符通过 cull（piano roll 缩小到底）
fn panorama_camera() -> CameraUniform {
    CameraUniform {
        scroll: [0.0, 0.0],
        zoom: [
            (BENCH_W as f32 - KEYBOARD_W) / TOTAL_TICKS,
            (BENCH_H as f32 - RULER_H) / KEY_COUNT as f32,
        ],
        viewport_size: [BENCH_W as f32, BENCH_H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [BENCH_W as f32, BENCH_H as f32],
        keyboard_width: KEYBOARD_W,
        ruler_height: RULER_H,
        max_key_index: (KEY_COUNT - 1) as f32,
        _padding: [0.0; 3],
    }
}

/// 局部放大相机：约 0.06% tick × 52 键可见 ⇒ cull 剔除绝大多数音符
fn zoomed_camera() -> CameraUniform {
    CameraUniform {
        zoom: [2.0, 20.0],
        ..panorama_camera()
    }
}

/// 提交单个 encoder 并**有界等待** GPU 完成，返回墙钟耗时。
fn submit_and_wait(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: wgpu::CommandEncoder,
) -> Duration {
    let start = Instant::now();
    let _ = queue.submit(Some(encoder.finish()));
    // 有界等待：禁止 timeout: None（无限阻塞会让基准在驱动异常时挂死）
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(60)),
    });
    start.elapsed()
}

/// 对 `encode` 重复 `iters` 轮，返回中位数耗时（**丢弃前 2 轮预热**：
/// 首轮含驱动惰性分配与时钟爬升，第二轮仍可能受电源状态影响）。
fn median_time(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    iters: usize,
    mut encode: impl FnMut(&mut wgpu::CommandEncoder),
) -> Duration {
    const WARMUP_ROUNDS: usize = 2;
    let mut samples: Vec<Duration> = Vec::with_capacity(iters);
    for round in 0..iters {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("note_perf_bench"),
        });
        encode(&mut encoder);
        let elapsed = submit_and_wait(device, queue, encoder);
        if round >= WARMUP_ROUNDS {
            samples.push(elapsed);
        }
    }
    samples.sort_unstable();
    samples
        .get(samples.len() / 2)
        .copied()
        .unwrap_or(Duration::ZERO)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// 有界回读 staging（复刻 depth_tests 的非阻塞轮询模式）。
fn read_u32(device: &wgpu::Device, queue: &wgpu::Queue, buffer: &wgpu::Buffer, offset: u64) -> u32 {
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("note_perf_readback"),
        size: 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("note_perf_readback_encoder"),
    });
    encoder.copy_buffer_to_buffer(buffer, offset, &staging, 0, 4);
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let _ = device.poll(wgpu::PollType::Poll);
        match rx.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(e)) => panic!("回读 map 失败: {e:?}"),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                assert!(Instant::now() < deadline, "回读 30s 未就绪");
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => panic!("回读回调丢失"),
        }
    }
    let value = {
        let data = slice.get_mapped_range();
        u32::from_ne_bytes([data[0], data[1], data[2], data[3]])
    };
    staging.unmap();
    value
}

/// 单档位测量结果（打印与断言共用）。
struct Row {
    count: usize,
    visible: u32,
    clear_ms: f64,
    cull_ms: f64,
    draw_ms: f64,
    total_ms: f64,
    /// 亚像素档位：`prepare_direct`（只写相机）+ 点图元直绘
    point_ms: f64,
}

impl Row {
    fn render_share(&self) -> f64 {
        if self.total_ms <= 0.0 {
            0.0
        } else {
            self.draw_ms / self.total_ms * 100.0
        }
    }
}

/// 测量一档实例数在给定相机下的三段耗时。
fn measure(gpu: &BenchGpu, count: usize, zoomed: bool, iters: usize) -> Row {
    let notes = synth_notes(count);
    // 与生产一致：钢琴卷帘的全量音符层是 onion_skin 渲染器（新渲染器实例，隔离状态）
    let mut renderer = NoteRenderer::new_onion_skin(&gpu.device, &gpu.queue, gpu.format);
    // 主音轨 = track_enc 1；无静音轨（与「主轨可见」生产态一致）
    renderer.set_view_state(&gpu.queue, 1, &[]);
    renderer.upload_instances(&notes, &gpu.device, &gpu.queue);
    let camera = if zoomed {
        zoomed_camera()
    } else {
        panorama_camera()
    };

    let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("note_perf_color"),
        size: wgpu::Extent3d {
            width: BENCH_W,
            height: BENCH_H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("note_perf_depth"),
        size: wgpu::Extent3d {
            width: BENCH_W,
            height: BENCH_H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    let clear_color = wgpu::Color {
        r: 0.05,
        g: 0.05,
        b: 0.05,
        a: 1.0,
    };
    let color_attachment = || wgpu::RenderPassColorAttachment {
        view: &color_view,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(clear_color),
            store: wgpu::StoreOp::Store,
        },
        depth_slice: None,
    };
    let depth_attachment = || wgpu::RenderPassDepthStencilAttachment {
        view: &depth_view,
        depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Discard,
        }),
        stencil_ops: None,
    };

    // ① 纯 clear（空白 attach 基线，用于判断小规模下的测量底噪）
    let clear_ms = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        let _pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_perf_clear"),
            color_attachments: &[Some(color_attachment())],
            depth_stencil_attachment: Some(depth_attachment()),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }));

    // ② cull pass（prepare_pass：重置 indirect + 写 viewport uniform + 全量 dispatch）
    let cull_ms = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        renderer.prepare_pass(enc, camera, &gpu.queue);
    }));

    // 可见数回读：确认相机档位语义（全景应≈全部可见，放大应远小于总数）
    let visible = read_u32(
        &gpu.device,
        &gpu.queue,
        renderer.indirect_buffer.inner(),
        4, // DrawIndirectArgs: vertex_count(4B) 之后是 instance_count
    );

    // ③ render pass（用上一步 cull 产出的可见列表 + indirect 参数，单测绘制段）
    let draw_ms = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_perf_draw"),
            color_attachments: &[Some(color_attachment())],
            depth_stencil_attachment: Some(depth_attachment()),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw(&mut pass, true, None);
    }));

    // ④ cull + render 同一 encoder（真实帧形状）
    let total_ms = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        renderer.prepare_pass(enc, camera, &gpu.queue);
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_perf_total"),
            color_attachments: &[Some(color_attachment())],
            depth_stencil_attachment: Some(depth_attachment()),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw(&mut pass, true, None);
    }));

    // ⑤ 亚像素档位：点图元直绘（不跑 cull，可见性判定在 vs_point 内）
    let point_ms = ms(median_time(&gpu.device, &gpu.queue, iters, |enc| {
        renderer.prepare_direct(camera, &gpu.queue);
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_perf_point"),
            color_attachments: &[Some(color_attachment())],
            depth_stencil_attachment: Some(depth_attachment()),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw_points(&mut pass, true, None);
    }));

    Row {
        count,
        visible,
        clear_ms,
        cull_ms,
        draw_ms,
        total_ms,
        point_ms,
    }
}

fn env_counts(gpu: &BenchGpu) -> Vec<usize> {
    let cap = gpu.max_instances(4); // 可见索引缓冲 4B/实例
    if let Ok(raw) = std::env::var("LUMINO_PERF_NOTES") {
        let parsed: Vec<usize> = raw
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }
    DEFAULT_COUNTS
        .iter()
        .copied()
        .filter(|c| *c <= cap)
        .collect()
}

fn env_iters() -> usize {
    std::env::var("ITERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v >= 2)
        .unwrap_or(DEFAULT_ITERS)
}

/// 打印一张表的表头（含硬件环境——卡片验收要求固化硬件口径）。
fn print_header(gpu: &BenchGpu, scene: &str, iters: usize) {
    eprintln!(
        "\n=== {scene} · adapter={} · {iters} 轮中位数（前 2 轮预热丢弃）===",
        gpu.adapter_info
    );
    eprintln!(
        "{:>12} {:>12} {:>8} {:>8} {:>8} {:>8} {:>9} {:>8} {:>9}",
        "notes", "visible", "clear", "cull", "draw", "cull+draw", "total", "render%", "point"
    );
}

fn print_row(r: &Row) {
    eprintln!(
        "{:>12} {:>12} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>9.2} {:>7.1}% {:>8.2}",
        r.count,
        r.visible,
        r.clear_ms,
        r.cull_ms,
        r.draw_ms,
        r.cull_ms + r.draw_ms,
        r.total_ms,
        r.render_share(),
        r.point_ms
    );
}

/// 面板瀑布流预览之外的另一种负载画像：**同 key 大量堆叠**（黑乐谱真实形态）。
///
/// 均匀铺满（`synth_notes`）测的是「图元数量」；本函数测的是「重叠深度」——
/// 每簇 `DENSE_CLUSTER_NOTES` 个同 key 音符按 `DENSE_TICK_SPACING` 递推铺开，
/// 全景缩放下它们会坍缩到同一两根像素柱上，形成几十~上百层叠加：
/// 这正是 PREF-005 所说「同 key 行音符在时间上大量重叠，同一屏幕像素柱叠加
/// 几十上百个 quad」的负载。
const DENSE_CLUSTER_NOTES: usize = 128;
/// 同簇内相邻音符的 tick 间距（1/16 音符）
const DENSE_TICK_SPACING: f32 = 120.0;

/// 生成稠密堆叠场景，返回 `(实例, 全曲 tick 跨度)`。
fn synth_notes_dense(count: usize) -> (Vec<NoteInstance>, f32) {
    let mut notes = Vec::with_capacity(count);
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for i in 0..count {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        // 每 128 个音符共享一个 key（簇内 tick 递推），簇间换 key —— 复刻黑乐谱
        // 「同一键位高速重复」+「跨键位密集跑动」两种叠加
        let cluster = i / DENSE_CLUSTER_NOTES;
        let within = i % DENSE_CLUSTER_NOTES;
        let key = 21 + ((cluster as u32).wrapping_mul(7) % 88);
        let tick =
            cluster as f32 * DENSE_CLUSTER_NOTES as f32 * 0.25 + within as f32 * DENSE_TICK_SPACING;
        notes.push(NoteInstance::new(
            tick,
            key as u8,
            NOTE_LEN,
            [0.2, 0.55, 1.0, 1.0],
            (1u32 << 16) | 1,
        ));
        let _ = state;
    }
    let span = notes
        .last()
        .map_or(NOTE_LEN, |n| n.start_length[0] + NOTE_LEN);
    (notes, span)
}

/// 全景相机（按给定全曲 tick 跨度把整首歌缩到屏内）。
fn panorama_camera_for(total_ticks: f32) -> CameraUniform {
    CameraUniform {
        scroll: [0.0, 0.0],
        zoom: [
            (BENCH_W as f32 - KEYBOARD_W) / total_ticks.max(1.0),
            (BENCH_H as f32 - RULER_H) / KEY_COUNT as f32,
        ],
        viewport_size: [BENCH_W as f32, BENCH_H as f32],
        canvas_offset: [0.0, 0.0],
        canvas_size: [BENCH_W as f32, BENCH_H as f32],
        keyboard_width: KEYBOARD_W,
        ruler_height: RULER_H,
        max_key_index: (KEY_COUNT - 1) as f32,
        _padding: [0.0; 3],
    }
}

/// PREF-005 的三条待对比路径。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FramePath {
    /// 改动前基线：alpha 混合管线 + compute cull + 可见索引绘制
    CullBlended,
    /// 只去掉混合：不透明管线 + compute cull
    CullOpaque,
    /// 本卡路径：不透明管线 + VS 内自判可见 + 按实例序直绘（无 compute cull）
    DirectOpaque,
}

/// 测量三条路径的**整帧**耗时（含各自的准备阶段），返回 `[mixed, cull, direct]`（ms）。
fn measure_frame_paths(
    gpu: &BenchGpu,
    notes: &[NoteInstance],
    camera: CameraUniform,
    iters: usize,
) -> [f64; 3] {
    let size = wgpu::Extent3d {
        width: BENCH_W,
        height: BENCH_H,
        depth_or_array_layers: 1,
    };
    let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vs_cull_ab_color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vs_cull_ab_depth"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let clear_color = wgpu::Color {
        r: 0.05,
        g: 0.05,
        b: 0.05,
        a: 1.0,
    };

    let mut renderer = NoteRenderer::new_onion_skin(&gpu.device, &gpu.queue, gpu.format);
    renderer.set_view_state(&gpu.queue, 1, &[]);
    renderer.upload_instances(notes, &gpu.device, &gpu.queue);

    // **双向轮转取最小值**：单项测量会被顺序效应污染（第一项常测得偏慢——
    // 时钟态/冷缓存），单轮顺序采样得出的「A 比 B 快 40%」可能纯属顺序假象。
    // 正序 + 逆序各测一轮，取各路径两轮最小值，消除顺序偏置。
    let mut out = [f64::INFINITY; 3];
    for reversed in [false, true] {
        let mut order = [
            FramePath::CullBlended,
            FramePath::CullOpaque,
            FramePath::DirectOpaque,
        ];
        if reversed {
            order.reverse();
        }
        for path in order {
            let slot = match path {
                FramePath::CullBlended => 0,
                FramePath::CullOpaque => 1,
                FramePath::DirectOpaque => 2,
            };
            // 混合基线：把横向 quad 管线换成混合变体（复刻改动前状态）
            if path == FramePath::CullBlended {
                renderer.rebuild_blended_quad_pipeline_for_bench(&gpu.device, gpu.format);
            }
            let elapsed = ms(median_time(
                &gpu.device,
                &gpu.queue,
                iters,
                |enc| match path {
                    FramePath::CullBlended | FramePath::CullOpaque => {
                        renderer.prepare_pass(enc, camera, &gpu.queue);
                        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("vs_cull_ab_cull_path"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &color_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(clear_color),
                                    store: wgpu::StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &depth_view,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(1.0),
                                        store: wgpu::StoreOp::Discard,
                                    }),
                                    stencil_ops: None,
                                },
                            ),
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        renderer.draw(&mut pass, true, None);
                    }
                    FramePath::DirectOpaque => {
                        renderer.prepare_direct(camera, &gpu.queue);
                        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("vs_cull_ab_direct_path"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &color_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(clear_color),
                                    store: wgpu::StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &depth_view,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(1.0),
                                        store: wgpu::StoreOp::Discard,
                                    }),
                                    stencil_ops: None,
                                },
                            ),
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        renderer.draw_direct(&mut pass, true, None);
                    }
                },
            ));
            out[slot] = out[slot].min(elapsed);
        }
    }
    out
}

/// PREF-005 A/B：混合+compute cull（改动前） vs 不透明+compute cull vs 不透明+VS cull。
///
/// 验收口径即本表的 `direct / blended - 1`：卡内目标是 zoom-out 全曲视图
/// **帧时间下降 ≥ 50%**。
#[test]
#[ignore = "GPU 基准：需显式 --ignored 运行，见模块头注释"]
fn bench_vs_cull_ab() {
    let gpu = BenchGpu::new();
    let counts = env_counts(&gpu);
    let iters = env_iters();
    assert!(!counts.is_empty(), "无可测档位（设备容量过小？）");

    for (view, zoomed) in [
        ("全景（全曲缩放到屏）", false),
        // zoom-in：cull 剔除率最高的档位（PREF-004 实测该档 cull 占总帧 56%）。
        // 本卡验收要求「zoom-in 渲染帧时间不回退」——直绘取消了 cull pass，
        // 该档预期为收益；此处即为该条验收的实测来源。
        ("放大（局部约 52 键）", true),
    ] {
        for (scene, dense) in [
            ("dense（同 key 堆叠 · 黑乐谱形态）", true),
            ("uniform（均匀铺满）", false),
        ] {
            eprintln!(
                "\n=== {view} · {scene} · adapter={} · {iters} 轮中位数（前 2 轮预热丢弃）===",
                gpu.adapter_info
            );
            eprintln!(
                "{:>12} {:>12} {:>12} {:>12} {:>10} {:>10}",
                "notes", "blended+cull", "opaque+cull", "opaque+直接", "去混合", "总降幅"
            );
            for &count in &counts {
                let (notes, span) = if dense {
                    synth_notes_dense(count)
                } else {
                    (synth_notes(count), TOTAL_TICKS)
                };
                let camera = if zoomed {
                    // 放大档：2 px/tick × 20 px/key（约 52 键可见）
                    CameraUniform {
                        zoom: [2.0, 20.0],
                        ..panorama_camera_for(span)
                    }
                } else {
                    panorama_camera_for(span)
                };
                let [blended, cull, direct] = measure_frame_paths(&gpu, &notes, camera, iters);
                eprintln!(
                    "{:>12} {:>11.2} {:>11.2} {:>11.2} {:>9.0}% {:>9.0}%",
                    count,
                    blended,
                    cull,
                    direct,
                    (1.0 - cull / blended.max(f64::EPSILON)) * 100.0,
                    (1.0 - direct / blended.max(f64::EPSILON)) * 100.0
                );
            }
        }
    }
}
///
/// 全景（缩小到底：全曲 tick + 全键可见）分段耗时。
///
/// 这是黑乐谱滚动拖拽的**主场景**：cull 几乎不剔除任何音符，
/// 且全部 quad 都是亚像素宽。
#[test]
#[ignore = "GPU 基准：需显式 --ignored 运行，见模块头注释"]
fn bench_note_layer_panorama_gpu_breakdown() {
    let gpu = BenchGpu::new();
    let counts = env_counts(&gpu);
    let iters = env_iters();
    assert!(!counts.is_empty(), "无可测档位（设备容量过小？）");

    print_header(&gpu, "panorama（缩小全景）", iters);
    let mut rows = Vec::new();
    for count in counts {
        let row = measure(&gpu, count, false, iters);
        print_row(&row);
        rows.push(row);
    }

    // 趋势说明：打印 ns/实例，用于判断 cull 是否线性于总音符数
    eprintln!("\n[perf] panorama 单位成本：");
    for r in &rows {
        let n = r.count as f64;
        eprintln!(
            "  notes={:>10}  cull={:>7.3} ns/inst   draw(可见)={:>7.3} ns/visible   draw(总)={:>7.3} ns/inst   point={:>7.3} ns/inst   提速={:>5.2}x",
            r.count,
            r.cull_ms * 1e6 / n,
            r.draw_ms * 1e6 / r.visible.max(1) as f64,
            r.draw_ms * 1e6 / n,
            r.point_ms * 1e6 / n,
            if r.point_ms > 0.0 {
                r.total_ms / r.point_ms
            } else {
                0.0
            }
        );
    }
}

/// 局部放大（约 0.06% tick × 52 键可见）分段耗时——cull 剔除率最高的场景。
#[test]
#[ignore = "GPU 基准：需显式 --ignored 运行，见模块头注释"]
fn bench_note_layer_zoomed_gpu_breakdown() {
    let gpu = BenchGpu::new();
    let counts = env_counts(&gpu);
    let iters = env_iters();
    assert!(!counts.is_empty(), "无可测档位（设备容量过小？）");

    print_header(&gpu, "zoomed（局部放大）", iters);
    for count in counts {
        print_row(&measure(&gpu, count, true, iters));
    }
}
