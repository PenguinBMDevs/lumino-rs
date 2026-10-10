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
mod benches;
mod frame_paths;
mod helpers;
mod measure;
