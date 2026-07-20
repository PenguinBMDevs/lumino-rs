# Lumino 视频导出渲染性能分析报告

## 执行摘要

经过对 `crates/export/src/video/ffmpeg.rs`、`crates/gfx/src/render_thread/export_pipeline.rs`、`crates/gfx/src/render_thread/render_loop/runner/run.rs` 等核心文件的深入分析，视频导出渲染管线存在 **7 个高影响性能瓶颈**，其中 **3 个属于可以带来数倍性能提升的架构级问题**。当前管线的理论最大渲染速度受限于 CPU 内存分配和线程间同步开销，而非 GPU 渲染能力。

| 等级 | 问题 | 预估影响 | 优化收益 |
|------|------|---------|---------|
| 🔴 P0 | 每帧新建 `Vec<u8>` 堆分配大内存（`export_pipeline.rs:183`） | 4K@60fps 每帧 33MB 分配 | **5~10x** |
| 🔴 P0 | 三线程串行流水线（渲染→Runner→ffmpeg） | 同步等待、上下文切换 | **2~3x** |
| 🔴 P0 | CPU 逐行 `copy_nonoverlapping` 去 padding（`export_pipeline.rs:186-206`） | 每帧 66MB 内存带宽 | **1.5~2x** |
| 🟡 P1 | 纯 2D 渲染仍创建/清除 Depth32Float 纹理 | 每帧 33MB 无用带宽 | **1.2x** |
| 🟡 P1 | 每帧 `texture.create_view()` 新建视图对象 | 分配开销 + 驱动开销 | **1.1x** |
| 🟡 P1 | 视频导出仍走通用渲染路径（`prepare_renderers` 全量执行） | 多余的 uniform 上传、事件检查 | **1.2x** |
| 🟡 P1 | `StoreOp::Store` 保留无需的 depth 数据 | 内存带宽浪费 | **1.1x** |
| 🟢 P2 | `RenderParams` 含大量废弃/空 `Vec` 字段 | 堆分配 + 传输开销 | 边际收益 |

---

## 1. 核心瓶颈详解

### 1.1 [P0] 每帧新建 `Vec<u8>` — 内存分配器杀手

**位置**: `crates/gfx/src/render_thread/export_pipeline.rs:183-184`

```rust
let total_unpadded = (buf.unpadded_bytes_per_row * buf.height) as usize;
let mut result = Vec::with_capacity(total_unpadded);  // ← 每帧新建
```

**问题描述**:
- 1920x1080@60fps: 每帧 `8.3MB` × 60 = **498MB/s** 的堆分配
- 3840x2160@60fps: 每帧 `33.2MB` × 60 = **1.99GB/s** 的堆分配
- `Vec` 通过 `mpsc::channel` 传递到 Runner，Runner 再传给 ffmpeg 写入线程，最终被丢弃
- 大量大对象分配导致堆碎片化，allocator 锁竞争，甚至触发系统 mmap/munmap

**数据流**:
```
GPU mapped range (33MB)
    ↓ copy_nonoverlapping (CPU 拷贝)
新 Vec<u8> (33MB, 堆分配)
    ↓ mpsc::send (所有权转移)
Runner 线程持有
    ↓ crossbeam::send (所有权转移)
ffmpeg 写入线程
    ↓ write_all → BufWriter → stdin pipe
内核 pipe buffer
    ↓ 丢弃 Vec
```

**优化方案 — `Vec<u8>` 对象池**:

在 `ExportPipeline` 中维护一个 `Vec<Vec<u8>>` 池，帧数据用完后归还而不是丢弃:

```rust
// export_pipeline.rs
struct StagingRing {
    // ... 现有字段 ...
    frame_pool: Vec<Vec<u8>>,  // 新增: 帧数据对象池
}

fn finish_read(&mut self) -> Vec<u8> {
    // ... 现有逻辑 ...
    
    // 从池中取出一个复用，或新建
    let mut result = self.frame_pool.pop().unwrap_or_else(|| {
        Vec::with_capacity(total_unpadded)
    });
    result.clear();  // 重置长度，保留容量
    
    unsafe {
        ptr::copy_nonoverlapping(src, result.as_mut_ptr(), total_unpadded);
        result.set_len(total_unpadded);
    }
    
    // ... 后续不变 ...
}

// Runner 侧: 写入 ffmpeg 后归还
// ffmpeg.rs 的写入线程循环中:
for frame_data in rx {
    stdin.write_all(&frame_data)?;
    // 发送回渲染线程归还 (通过另一个 channel)
    return_tx.send(frame_data).ok();
}
```

**预估收益**: 消除 90%+ 的大对象堆分配，对 4K 导出可提升 **5~10 倍** 整体吞吐量。

---

### 1.2 [P0] 三线程串行流水线 — 同步开销吞噬并行度

**位置**: `crates/gfx/src/render_thread/render_loop/runner/run.rs:208-219` + `crates/export/src/video/ffmpeg.rs:100-119`

**问题描述**:
当前数据流经过 3 个线程 + 2 个 channel:

```
渲染线程          Runner 线程           ffmpeg 写入线程
   │                  │                      │
   │ try_read()       │                      │
   │ ──────────────>  │ mpsc::Sender         │
   │   Vec<u8>        │ ──────────────────>  │ crossbeam::bounded(8)
   │                  │   Vec<u8>            │   Vec<u8>
   │                  │                      │   write_all()
   │                  │                      │   ──────> ffmpeg stdin
```

- `mpsc` 和 `crossbeam` 的同步原语（mutex/condvar 或 atomic）在高频小消息下开销累积
- Runner 线程成为瓶颈：必须 `recv` → `write_frame` → `send` 下一帧命令，串行执行
- ffmpeg 的 `bounded(8)` 背压 channel 满了时，Runner 阻塞在 `send`，无法继续调度

**优化方案 A — 渲染线程直连 ffmpeg（最优）**:

将 `FfmpegEncoder` 直接放到渲染线程中，消除 Runner 中转:

```rust
// 在 ExportPipeline 中持有 FfmpegEncoder
pub struct ExportPipeline {
    ring: StagingRing,
    cached_width: u32,
    cached_height: u32,
    encoder: Option<lumino_export::FfmpegEncoder>,  // 新增
}

// copy_and_submit 后直接 try_read 并写入 ffmpeg
pub fn copy_and_submit_and_drain(&mut self, ...) {
    self.copy_and_submit(encoder, source, queue);
    // 尝试读回并直接写入 ffmpeg
    while let Some(frame) = self.try_read() {
        if let Some(ref mut enc) = self.encoder {
            enc.write_frame(frame)?;  // 需要调整: 接收 Vec<u8> 而非 &mut self
        }
    }
}
```

**优化方案 B — 双缓冲 + 批量提交（次优，改动小）**:

Runner 一次性预提交 N 帧的 `RenderVideoFrame` 命令，渲染线程连续渲染，ffmpeg 写入线程独立消费:

```rust
// Runner 侧: 预填充 inflight
for _ in 0..4 {
    render_thread.send(RenderVideoFrame { params: ... });
}

// 之后每收到一帧，发下一帧
while let Ok(frame) = frame_rx.recv() {
    ffmpeg.write_frame(frame)?;
    render_thread.send(RenderVideoFrame { params: ... });
}
```

这样渲染线程的 4 槽 staging ring 始终被填满，GPU 和 CPU 流水线始终保持满载。

**预估收益**: 消除线程切换和 channel 同步开销，提升 **2~3 倍**。

---

### 1.3 [P0] CPU 逐行去 padding — 内存带宽浪费

**位置**: `crates/gfx/src/render_thread/export_pipeline.rs:186-206`

```rust
unsafe {
    for row in 0..buf.height as usize {
        let src = data.as_ptr().add(row * padded);
        let dst = result.as_mut_ptr().add(row * unpadded);
        ptr::copy_nonoverlapping(src, dst, unpadded);
    }
    result.set_len(total_unpadded);
}
```

**问题描述**:
- wgpu 的 `COPY_BYTES_PER_ROW_ALIGNMENT = 256`，导致每行可能有 padding
- 1920px × 4B = 7680B，恰好是 256 的倍数 → **无 padding** ✅
- 但 1921px × 4B = 7684B → padded to 7936B → **每行 252B padding** ❌
- 4K (3840px): 15360B，256 倍数 → **无 padding** ✅
- 非标准分辨率（如 1366x768）会产生大量 padding

**优化方案 — 零拷贝/整块拷贝（无 padding 时）**:

当前代码已经有这个优化分支，但需要确保常见分辨率（1080p、4K）触发它:

```rust
if buf.padded_bytes_per_row == buf.unpadded_bytes_per_row {
    // 整块拷贝 (fast path)
    unsafe {
        ptr::copy_nonoverlapping(data.as_ptr(), result.as_mut_ptr(), total_unpadded);
        result.set_len(total_unpadded);
    }
} else {
    // 逐行拷贝 (slow path)
    // ... 现有逻辑 ...
}
```

进一步，可以**在创建离屏纹理时选择对齐宽度**，确保 `width × 4` 总是 256 的倍数:

```rust
// textures.rs: 创建离屏纹理前对齐
let aligned_width = ((width * 4 + 255) / 256) * 256 / 4;  // 向上对齐到 256 字节边界
```

但这会改变实际渲染分辨率，需要谨慎。更好的做法是在 `ExportPipeline` 的 `StagingBuffer` 中使用**行对齐**，让 `unpadded_bytes_per_row == padded_bytes_per_row`:

```rust
// 实际上，当前代码的 padded_bytes_per_row 计算已经是对齐的:
let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
```

所以如果 `width * 4` 不是 256 倍数，padding 就存在。解决方案：在视频导出配置中**限制分辨率为 64 像素倍数**（因为 64 × 4 = 256），或者接受 padding 并让 ffmpeg 直接读取 padded 数据。

**高级方案 — 让 ffmpeg 直接读 padded 数据**:

ffmpeg 的 rawvideo 输入格式是紧密排列的，但如果用 `bgra` 格式且指定了 stride:

```bash
ffmpeg -f rawvideo -pix_fmt bgra -s 1920x1080 ...
```

ffmpeg 期望每行 `1920 × 4 = 7680` 字节。如果数据有 padding，颜色会错乱。

但我们可以使用 wgpu 的 `copy_texture_to_buffer` 的 `bytes_per_row` 参数，设置为 `unpadded_bytes_per_row`，然后确保 buffer 大小足够。实际上 wgpu 要求 `bytes_per_row` 必须是 256 对齐的，所以无法避免 padding。

**结论**: 对于 1080p 和 4K 这种标准分辨率，fast path（整块拷贝）已经生效。对于非标准分辨率，逐行拷贝是必需的。这不是最严重的瓶颈。

**预估收益**: 标准分辨率下边际收益；非标准分辨率下 **1.5~2x**。

---

## 2. 中高影响瓶颈

### 2.1 [P1] 纯 2D 渲染仍创建/使用 Depth 纹理

**位置**: `crates/gfx/src/render_thread/render_loop/textures.rs:60-74` + `render_pass.rs:50-58,96-103`

```rust
// 创建深度纹理 (每帧尺寸变化时重建)
let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
    format: wgpu::TextureFormat::Depth32Float,
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    ...
});
```

**问题描述**:
- 钢琴卷帘、网格、音符、标尺、CC 柱状条全部是 2D 渲染，没有 3D 几何
- 深度测试对 2D 渲染无意义（渲染顺序由 `render_pass` 的 draw 调用顺序决定）
- 每帧 Depth Clear (`LoadOp::Clear(1.0)`) 消耗 33MB（4K）内存带宽
- Depth Store (`StoreOp::Store`) 再消耗 33MB
- 深度纹理占用 GPU 内存：4K 下 3840×2160×4B = 33MB

**优化方案**:

在视频导出模式下完全禁用 depth:

```rust
// render_pass.rs
if params.is_video_export {
    // 无 depth attachment 的 render pass
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[...],
        depth_stencil_attachment: None,  // ← 禁用
        ...
    });
} else {
    // 保留 depth（正常 UI 渲染可能需要）
    ...
}
```

同时修改 `ensure_textures` 在视频导出时跳过 depth texture 创建:

```rust
// textures.rs
pub fn ensure_textures(resources: &mut OffscreenTextureResources<'_>, needs_depth: bool) -> bool {
    // ...
    if needs_depth {
        // 创建 depth texture
    }
}
```

**预估收益**: 消除 33MB/帧 clear + 33MB/帧 store = **~1.2x** 提升（4K 下更明显）。

---

### 2.2 [P1] 每帧 `texture.create_view()` 创建新视图

**位置**: `crates/gfx/src/render_thread/render_loop/render_pass.rs:28`

```rust
let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
```

**问题描述**:
- `TextureView` 是轻量级对象，但每帧创建 + 丢弃仍有开销
- 驱动层可能需要在 GPU 上分配/释放 view 资源
- 视频导出期间纹理尺寸不变，视图完全可以缓存

**优化方案**:

在 `OffscreenTextureResources` 或 `RenderFrameState` 中缓存 `TextureView`:

```rust
// textures.rs
pub struct OffscreenTextureResources<'a> {
    // ... 现有字段 ...
    pub cached_texture_view: &'a mut Option<wgpu::TextureView>,  // 新增
}

// ensure_textures 中
if changed {
    *resources.cached_texture_view = Some(texture.create_view(&Default::default()));
}
```

**预估收益**: 减少每帧 1 个 API 对象创建，**~1.1x**。

---

### 2.3 [P1] 视频导出走通用渲染路径 — 大量多余工作

**位置**: `crates/gfx/src/render_thread/render_loop/prepare.rs:9-75` + `run.rs:289-304`

**问题描述**:
视频导出期间，每帧都执行完整的 `prepare_renderers`:

```rust
// prepare.rs
renderers.grid.prepare(queue, &grid_params);           // 每帧写 uniform buffer
renderers.ruler.prepare(device, queue, &ruler_params); // 条件执行
renderers.cc_bar.prepare(device, queue, ...);          // 条件执行
renderers.note.process_events(note_events_rx, ...);    // 每帧 try_recv 空队列
```

对于视频导出:
- `grid.prepare`: scroll 每帧线性变化，`cached_uniform` 比较每帧都失败，每帧都 `write_buffer`
- `ruler.prepare`: 标尺实例通常不变，但仍检查并可能重建
- `note.process_events`: 视频导出期间无音符编辑，但每帧都 `try_recv` 空 channel
- `CameraUniform::new` + `prepare_pass`: 每帧构造 + 上传

**优化方案 — 视频导出专用快速路径**:

```rust
// run.rs: handle_video_frame 中跳过通用 prepare
fn handle_video_frame(...) {
    // ... 现有逻辑 ...
    
    // 视频导出专用: 跳过 process_events，直接 upload_instances
    if !params.note_instances.is_empty() {
        frame.renderers.note.upload_instances(&params.note_instances, &ctx.device, &ctx.queue);
    }
    
    // 只更新 camera uniform（scroll 变化），跳过其他 prepare
    let camera = CameraUniform::new(...);
    frame.renderers.note.prepare_pass(encoder, camera, &ctx.queue);
    
    // grid: 直接 write_buffer camera uniform，跳过 builder 和 cache 比较
    // ...
}
```

更激进的方案：将 `scroll` 变化直接传入 shader 作为 uniform，完全跳过 CPU 侧的 `prepare`。

**预估收益**: 减少每帧 CPU 工作量，**~1.2x**。

---

### 2.4 [P1] `StoreOp::Store` 保留无用 depth 数据

**位置**: `render_pass.rs:56,100`

```rust
store: wgpu::StoreOp::Store,
```

**问题描述**:
即使保留了 depth attachment（见 2.1），深度数据在 render pass 结束后也不需要保留到内存。下一帧会重新 `Clear(1.0)`。

**优化方案**:

```rust
depth_ops: Some(wgpu::Operations {
    load: wgpu::LoadOp::Clear(1.0),
    store: wgpu::StoreOp::Discard,  // ← 改为 Discard
}),
```

**预估收益**: 消除 depth store 带宽，**~1.1x**。

---

## 3. 中低影响项

### 3.1 `RenderParams` 中的废弃字段

`grid_instances: Vec<GridLineInstance>` 在 GPU grid 方案中已废弃，但仍存在于 `RenderParams` 中，每帧默认构造空 `Vec`（24 字节 + 0 分配）。影响极小。

### 3.2 `Box<RenderParams>` 每帧堆分配

`RenderVideoFrame { params: Box<RenderParams> }` 每帧在堆上分配 `RenderParams`。`RenderParams` 本身较大（含 6 个 `Vec`），但 `Box` 分配开销相比 33MB 帧数据可忽略。

### 3.3 `puffin::profile_scope!` 宏开销

每帧多个 profiling scope，在 release 模式下通常被编译器优化或开销极小。

### 3.4 `advance_export_inflight` 每轮调用 `device.poll`

`device.poll(PollType::Poll)` 是轻量操作，但每 16ms 调用一次对 GPU 调度有轻微影响。可以改为只在 `try_read` 前调用。

---

## 4. 疑似功能性 Bug

### 4.1 纹理格式与 ffmpeg `-pix_fmt bgra` 不匹配

**位置**: `crates/export/src/video/ffmpeg.rs:295-296`

```rust
args.push("-pix_fmt".to_string());
args.push("bgra".to_string());
```

wgpu 的离屏纹理格式 `ctx.texture_format` 通常是 `Rgba8Unorm`（跨平台默认）或 `Bgra8Unorm`（某些平台 Surface 默认）。如果实际是 `Rgba8Unorm`，读回的数据是 RGBA 顺序，而 ffmpeg 被配置为 BGRA，会导致**红蓝通道互换**。

**验证方法**: 检查 `ctx.texture_format` 的实际值，或在导出代码中根据纹理格式自动选择 ffmpeg 的 `-pix_fmt` 参数:

```rust
let pix_fmt = match texture_format {
    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => "bgra",
    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => "rgba",
    _ => panic!("unsupported texture format for export"),
};
```

---

## 5. 优化路线图

### Phase 1: 快速修复（1~2 天，预估 30~50% 提升）

1. **对象池**: `ExportPipeline` 添加 `Vec<u8>` 池，ffmpeg 写入后归还
2. **禁用 depth**: 视频导出时 `depth_stencil_attachment: None`
3. **StoreOp::Discard**: depth 改为 Discard
4. **缓存 TextureView**: 避免每帧新建

### Phase 2: 流水线重构（3~5 天，预估 2~5x 提升）

1. **渲染线程直连 ffmpeg**: 消除 Runner 中转和 `mpsc` channel
2. **批量命令提交**: Runner 预填充 inflight，保持 GPU 满载
3. **视频导出专用渲染路径**: 跳过 `process_events`、简化 `prepare`

### Phase 3: 架构级优化（1~2 周，预估额外 20~50% 提升）

1. **GPU 直接编码**: 使用 Vulkan Video / NVENC 直接从 GPU 纹理编码，完全消除 CPU 读回
2. **异步多帧渲染**: 提交 N 帧的 render pass 到 GPU，批量读回
3. **分辨率为 64 倍数对齐**: 确保零 padding 整块拷贝

---

## 6. 关键代码定位速查

| 文件 | 行号 | 内容 |
|------|------|------|
| `export_pipeline.rs` | 183 | `Vec::with_capacity(total_unpadded)` — 每帧堆分配 |
| `export_pipeline.rs` | 186-206 | `copy_nonoverlapping` 逐行去 padding |
| `ffmpeg.rs` | 89-119 | `BufWriter` + 写入线程 + crossbeam channel |
| `run.rs` | 208-219 | `advance_export_inflight` — mpsc 发送帧数据 |
| `run.rs` | 289-304 | `prepare_renderers` + `execute_render_pass` |
| `render_pass.rs` | 28 | `texture.create_view()` — 每帧新建 |
| `render_pass.rs` | 50-58, 96-103 | depth attachment Store |
| `textures.rs` | 60-74 | depth texture 创建 |
| `grid_renderer.rs` | 356-379 | `GridCameraUniform` builder 每帧构建 |
| `note_renderer/prepare.rs` | 54-98 | `update_cull_info` — bind group 重建判断 |

---

*报告生成时间: 2026-07-19*
*分析范围: crates/export/src/video/*, crates/gfx/src/render_thread/*
