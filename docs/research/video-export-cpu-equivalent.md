# 视频导出 · CPU 等价方案调研与实现方案

> 范围：Lumino 视频导出（`NoteRectangle` 钢琴卷帘成片）新增一条**纯 CPU 渲染后端**，
> 与现有 GPU 后端**视觉一致**（口径见 §6.0：2 LSB 容差 + 覆盖/结构一致，**不追逐字节**，依据用户 2026-10-31 决策），
> 整链路导出吞吐 **≥ 60 fps**（1080p）。
> 统一数据源：`Arc<MidiDocument>`（唯一权威源，禁止第二份音符镜像）。
>
> 方法：逐路径读源码定位「已经有什么 / 缺什么 / 哪里有坑」，每个结论带 `文件:行号`。
> 关联文档：
> - `docs/video_export_perf_analysis.md`（2026-07-19 GPU 导出性能报告，本文复核其已闭环项）
> - `docs/2026-10-17-数据源冗余审计.md`（单一权威源口径）
> - `examples/cpu-onionskin/README.md`（CPU 光栅参考实现，实测 131.8 fps）

---

## 0. 结论速览

| 判断 | 结论 | 依据 |
|---|---|---|
| CPU 等价方案是否已有基础？ | **已有生产级内核**：`crates/editor/cpu-onion`（`lumino-cpu-onion`）已把 `examples/cpu-onionskin` 抽成正式 crate（SIMD/band/增量窄条/区域重绘/Exact+density LOD + 集成基准），但**只覆盖「洋葱皮」一层**（非当前轨、透明背景、canvas-only 坐标） | `crates/editor/cpu-onion/src/lib.rs:1-58`；`src/renderer.rs:1-120`；`tests/perf_1080p.rs` |
| 像素级一致可实现吗？ | **可**。全仓 shader **零 `fwidth`/`dpdx`/`dpdy`**、**零 MSAA**（`sample_count: 1`），几何全是轴对齐 quad → 无导数、无多重采样，栅格化规则可完整复刻 | `grep` 全 shader 目录；`crates/editor/gfx/src/pipeline.rs:145` |
| 现在就能对上吗？ | **先修 3 个先决缺陷**。①**已修复**：导出主音符层从不 dispatch cull ⇒ 间接绘制参数恒为 0 ⇒ **一个音符都不画**（真机单元测试取证，覆盖像素 0）；②导出无「主音轨」概念，与预览口径不同；③`RenderParams` 必须两后端共用同一份。**仍开放**：导出 pass 无 depth ⇒ 重叠区赢家理论上可逐帧翻转 | §2.3-①；`crates/editor/gfx/src/note_renderer/export_path_tests.rs` |
| ≥60fps 能达成吗？ | **渲染腿能（余量 5~8 倍），编码器是真正的天花板**。实测 libx264 preset=medium（当前默认）只有 **62.6 fps**，veryfast 89.5，NVENC 112.6 | `.ai-context/sessions/2026-09/2026-09-18-video-export-gpu-util-analysis.md:11-18` |
| 最大的性能杠杆 | **导出是单调前滚 + `zoom_x` 恒定** ⇒ 每帧只需重画新露出的 **~4~20 px 窄条**，其余按整数像素平移复用（该路径在 `cpu-onionskin` 已有逐像素回归测试） | `examples/cpu-onionskin/README.md:87-104`；`src/app/scroll_tests.rs` |
| 单帧预算实测推算 | ≈ **2.1 ms/帧**（位移 0.7 + 窄条 0.3 + 覆盖层 0.2 + 管道写 0.9）→ 渲染腿 ≈ 470 fps | §4.2 |

**一句话**：不是从零造渲染器，也不是照搬示例——**`lumino-cpu-onion` 已经提供了栅格/调度内核（洋葱皮层）**，本方案要做的是把它从「洋葱皮专用子集」扩到「整帧导出」（补主音轨层、不透明背景、网格/标尺、`RenderParams` 几何口径），先修掉 3 个 GPU 侧不确定性，再复用现有导出管线的帧通道与 ffmpeg 直灌。

---

## 1. 现状盘点：视频导出现在到底怎么跑

### 1.1 数据流（实测链路，非推测）

```
用户点击导出
  └─ handle_start_video_export                    src/runner/menu/window/video_export_handler.rs:28
       ├─ 可重入守卫（audio_export_control / video_export_running）  :35-47
       ├─ 事件层枚举 → 导出层枚举（总映射，无字符串解析）            :74-103
       ├─ 取 surface 纹理格式 → input_pix_fmt（bgra/rgba）           :120-126
       ├─ 起 std::thread("video-render")                            :172-174
       └─ (*document).clone() → Arc<MidiDocument> 快照（块级 COW）   :233-245
             ↓
run_video_export_task（同一线程）                .../video_export_handler/memory_task/task.rs:39
       ├─ FfmpegEncoder::new(config, input_pix_fmt)                 :79
       ├─ send_initial_render_commands（建 staging ring + 导出专用渲染器）
       ├─ total_frames = ceil(duration_secs × fps)                  :107
       └─ FramePipeline::run（预填充 4 帧 → 主循环 → drain）         :254-305
             ├─ enqueue_frame(frame_idx) → enqueue_memory_frame     .../memory_task/enqueue.rs:10
             │     ├─ tick = seconds_to_tick(frame_idx/fps)          :15-17
             │     ├─（NoteRectangle）update_playback_key_colors     :23-30
             │     └─ RenderVideoFrame{ params } → 渲染线程命令      :279-289
             └─ process_frame(frame_data) → composite_and_encode_frame
                                                                    .../video_export_handler/composite.rs:41
                   ├─ composite_keyboard + composite_ruler_numbers  :76-87
                   └─ encoder.write_frame(data) → ffmpeg stdin      :138
             ↓
渲染线程（GPU）
  handle_video_frame            crates/editor/gfx/.../render_loop/runner/video_export.rs:43
       ├─ ensure_textures(.., false)  ← 无 depth attachment        :111
       ├─ 首帧全量 upload_instances / 或直绑 onion 常驻缓冲          :122-141
       ├─ prepare_renderers + execute_render_pass（grid→onion→note→ruler→cc）
       ├─ pipeline.copy_and_submit（copy_texture_to_buffer + submit + map_async）
       └─ try_read / wait_read → frame_tx  → Runner
             ↓
ExportPipeline（4 槽 staging ring，去 padding，帧对象池）
                                          crates/editor/gfx/.../export_pipeline/staging.rs:24-355
             ↓
FramePipeline 主循环（每处理一帧立即补发下一帧，PIPELINE_DEPTH = 4）
                                                    .../video_export_handler/pipeline.rs:79
```

### 1.2 关键事实（决定方案形态的 6 条）

| # | 事实 | 证据 |
|---|---|---|
| F1 | **音符只在首帧全量上传一次**，之后每帧只发 uniforms；逐帧用 `collect_all=false` 跳过收集 | `render_params/note_rectangle.rs:22-37`；`enqueue.rs:247-277` |
| F2 | 稳态下 `visible_notes` / `note_instances` **为空**，`sort_visible_notes` / `pack_note_instances` 空转 | `render_params/note_rectangle_params.rs:46-54` |
| F3 | 导出 pass **没有 depth attachment**，导出专用渲染器按 `needs_depth=false` 创建 ⇒ `direct_pipeline` 恒为 `None`，`draw_direct` **回退 compute cull** | `render_loop.rs:79-98`；`note_renderer/init.rs:195-206`；`note_renderer/draw.rs:46-50` |
| F4 | `cull.wgsl` 用 **workgroup 抢占式 `atomicAdd`** 分配可见实例输出槽位 ⇒ **输出顺序帧间不稳定** | `shaders/cull.wgsl:96-120` |
| F5 | GPU→CPU 读回是 **4 槽 ring + 异步 map + 对象池**，已相当优化；剩余成本主要在 GPU 渲染本身 | `export_pipeline.rs:75-128`；`staging.rs:270-337` |
| F6 | 帧数据以 **BGRA 原始字节直灌 ffmpeg stdin**，YUV 转换与封装全在 ffmpeg 侧 ⇒ **像素一致性只需在「送进 stdin 的字节」上成立**，编码/封装天然同一 | `crates/tools/export/src/video/ffmpeg.rs:123-144`；`ffmpeg/args.rs` |

> **F6 是整份方案的地基**：像素级一致的验收面 = 「同一帧号下，两条后端交给 ffmpeg 的 `Vec<u8>` 逐字节相等」。

### 1.3 已有 6 种渲染模式，其中 3 种本来就是 CPU 渲染

| 模式 | 后端 | 帧来源 | 证据 |
|---|---|---|---|
| `NoteRectangle` | GPU | 离屏渲染 + 读回 | `video_export_handler.rs:193-199` |
| `Waterfall` | GPU compute | `waterfall.wgsl` | 同上 |
| `MIDITrail` | GPU 3D | `miditrail_3d.wgsl` | 同上 |
| `NoteCounter` | **CPU** | `render_counter_frame`，BGRA 直出 | `video_export_handler.rs:183-188, 200-210` |
| `DataCurve` | **CPU** | `render_data_curve_frame` | 同上 |
| `MidiConsole` | **GPU 或 CPU 可选** | `render_backend: {Gpu, Cpu}` | `enqueue.rs:207-236` |

**`MidiConsole` 已有 `render_backend` 双后端先例** —— 本方案沿用同一形态，不发明新范式。

### 1.4 顺带发现的既有缺陷（与本方案相关，建议同批修）

调研中命中 3 处**现有代码里就存在、与本方案直接相关**的问题，不属于本方案引入：

| # | 现象 | 证据 | 与本方案的关系 |
|---|---|---|---|
| B1 | **CPU 渲染模式仍会创建整套 GPU 回读管线**：`send_initial_render_commands` 无条件发 `StartVideoExport`，渲染线程照常建 4 槽 staging ring（1080p ≈ 33 MB）与整套导出渲染器，而这些模式**永远不发 `RenderVideoFrame`** | `memory_task/task.rs:92-101`；`render_loop/runner/video_export/common.rs:18-33` | CPU 后端必须**显式跳过**这一步（§5.3），否则白占 ~33 MB×4 与一次管线编译 |
| B2 | **预览缩略图色道错位**：预览分支对所有模式无条件做 BGRA→RGBA 交换，但 Waterfall / MIDITrail 的帧来自 `Rgba8Unorm` | `video_export_handler/composite.rs:110-135`（`:119` `downscale_bgra_to_rgba`、`:124` `swap(0,2)`）vs `waterfall_renderer/init.rs:54` | 不影响成片字节，但会让**导出预览**红蓝互换；加 `is_gpu_compute_style` 判断即可 |
| B3 | **未知纹理格式静默兜底 `"bgra"`**：`_ => "bgra"` | `video_export_handler.rs:122-126` | 若未来出现 `Rgba16Float` 等格式会静默错色；建议改为显式错误 |

> B2/B3 与像素一致性无关（不影响送进 ffmpeg 的字节），但会干扰「所见即所得」的验收判断，建议随本方案一起修。

---

### 1.5 地面实况更新：`lumino-cpu-onion` 已经存在（2026-10-31 复核）

调研过程中发现仓库里已有**生产级 CPU 光栅 crate**，方案的起点必须据此校正：

```
crates/editor/cpu-onion/            crates/editor/cpu-onion/src/
  Cargo.toml   依赖: lumino-midi-model, lumino-extras, tokio   （不依赖 gfx/wgpu）
  src/lib.rs           58 行   crate 契约与导出口径
  src/renderer.rs     371 行   CpuOnionRenderer（tokio 多 band + 增量）
  src/renderer/strip.rs        render_strip（水平窄条增量）
  src/renderer/region.rs       render_region（脏区重绘）
  src/renderer/output.rs       RGBA8 sRGB 输出（Arc 快照 / COW）
  src/renderer/overview.rs     总览 LOD
  src/band.rs 276 / notes.rs 330 / simd.rs 317 / compose.rs 252 / buffer.rs / slot.rs
  src/camera.rs 192  CanvasCamera（canvas-only 坐标）
  src/color.rs 170   NoteStyle（调色板 + 边框 ×0.4 + sRGB 编码）
  src/index.rs 354   TrackIndex（长音符 / lookback / 小节计数）
  src/density/       密度金字塔 + 彩色覆盖光栅 LOD（约 1700 行）
  tests/  onion_render 365 / region_render 286 / perf_1080p 286 / zoom_bench 237 /
          style_constants 71 / overview_lod 100
```

**它已经具备的（直接可用）**：SIMD 行填充与 owner 掩码填、多 band 并行、**增量窄条 + 整数像素平移**、脏区重绘、调色板与 `×0.4` 描边、sRGB 编码、`TrackIndex` 长音符索引、1080p 基准测试。

**它明确不做的（见 `lib.rs:1-35` 原文）**：

| 不做 | 原文依据 | 对视频导出的影响 |
|---|---|---|
| 只画「非当前轨 + 非静音轨」 | `lib.rs:4` | **主音轨层缺失**，导出必须补 |
| 输出透明背景（alpha=0） | `lib.rs:5, 22-23`；`frame.rs:9-10` | 导出需要不透明帧 + 背景色 |
| canvas-only 坐标（无 `keyboard_width` / `ruler_height`） | `lib.rs:18-22` | 与 `RenderParams` 几何口径不同，需适配层 |
| 不画键盘 / 标尺 / 网格 / 背景 | `lib.rs:14` | 网格层与标尺层**仍需新写**（§5.5） |
| Host 当前走整帧重绘 | `renderer/strip.rs:1` | 增量窄条已实现但**未接线**，导出正好可以用上 |

⇒ 结论修正：不是「新建 crate」，而是**在 `lumino-cpu-onion` 之上补齐「整帧导出后端」**。已在用的下游：`crates/editor/ui`（`Cargo.toml:26`）与 `crates/editor/gfx` 的 `cpu_onion_renderer`（`lib.rs:16, 65`，把 CPU 洋葱皮帧当纹理上传叠加）。

---

## 2. 需求拆解：像素级一致到底需要对齐什么

### 2.1 分解原则：共享一切，只复刻光栅

两条后端**必须**共用同一份输入，否则像素一致无从谈起：

| 层 | 是否共享 | 现有代码 |
|---|---|---|
| `MidiDocument` → 可见音符集合 | ✅ 共享（同一 `Arc` 快照） | `video_export_handler.rs:233-245` |
| tick → 帧号时间基 | ✅ 共享 | `video_export.rs:75-95`（`seconds_to_tick`） |
| 排序键 | ✅ 共享 | `render_params/helpers.rs:66-96`（`sort_visible_notes`） |
| `NoteInstance` 打包（含 `border_width`） | ✅ 共享 | `render_params/helpers.rs:100-115`；`note_renderer/types.rs:36-42, 93-99` |
| 每帧 `RenderParams`（scroll/zoom/canvas/scissor） | ✅ 共享（**必须**，见 §2.3-③） | `render_params/note_rectangle_params.rs:11-76` |
| ffmpeg 编码参数 | ✅ 共享 | `crates/tools/export/src/video/config.rs:318-335` |
| **顶点变换 → 光栅化 → 片元着色** | ❌ 必须各自实现，且逐位对齐 | GPU: `shaders/*.wgsl`；CPU: 待写 |

### 2.2 逐层等价清单（需要 CPU 复刻的全部视觉层）

导出 pass 的绘制顺序固定（`render_pass.rs:154-274`）：

| # | 层 | shader | 输入 | CPU 复刻难度 | 备注 |
|---|---|---|---|---|---|
| 1 | 清屏 | `LoadOp::Clear(background_color)` | `[f64;4]` | 极低 | `render_pass.rs:56-61, 162` |
| 2 | 背景网格（全屏） | `infinite_grid.wgsl` | camera uniform + 拍号表 | **低（可分离）** | 见 §2.2.1 |
| 3 | 洋葱皮音符 | `onion_note.wgsl` | 常驻实例缓冲 | 低 | 与音符同构 |
| 4 | 纹理瀑布流 | — | — | **不适用** | 导出传 `Vec::new()`（`render_loop/runner/video_export.rs:114`） |
| 5 | 主音符层 | `note.wgsl` | 常驻实例缓冲 + cull 输出 | 低~中 | 见 §2.5 |
| 6 | 标尺 | `ruler.wgsl` | 实例化 quad | 极低 | 纯色 + alpha 混合 |
| 7 | CC 柱状条 | `cc_bar.wgsl` | — | **不适用** | 导出 `velocity_panel_rect = None` |
| 8 | 键盘贴图 | （无 shader） | — | **已由 CPU 完成** | `keyboard/texture.rs:13-179` |
| 9 | 小节号数字 | （无 shader） | — | **已由 CPU 完成** | `video_export.rs:137-209` |

#### 2.2.1 网格 shader 是可分离的（关键性能发现）

`infinite_grid.wgsl:191-295` 的 `fs_main` 结构：

```
margin discard  →  bg_color(key)   ← 只依赖 screen_y
                   ↓
X 分支（小节/拍/半拍/6 档细分）  ← 只依赖 world_tick = f(screen_x)
   命中 → mix(bg, line_color, alpha)
                   ↓
Y 分支（琴键分隔线）             ← 只依赖 key_f32 = f(screen_y)
                   ↓
兜底 return bg_color
```

**结论**：`color(x, y)` = `X 层(x)` 优先，否则 `Y 层(y)`，否则 `bg(y)`。⇒ 每帧只需算 **W 次 X 层 + H 次 Y 层**，再按行填充，复杂度从 `O(W·H)`（200 万次）降到 `O(W+H)`（3 千次）+ 一次 8 MB 行填充。这条把网格从「最贵的层」变成「最便宜的层」。

### 2.3 三个必须先修的先决缺陷（不修，像素级一致无从验收）

#### ① GPU 导出**根本不画音符**（P0 已取证并修复）＋ 重叠序不确定（仍待修）

**取证（2026-10-31，真机 GPU 单元测试，非推断）**：

新增 `crates/editor/gfx/src/note_renderer/export_path_tests.rs`，5 条测试全部通过，其中两条是决定性的：

| 测试 | 断言 | 结果 |
|---|---|---|
| `prepare_direct_leaves_indirect_args_empty` | 导出路径调用的 `prepare_direct` 之后，`indirect_buffer` 前 32 字节仍是 `vertex_count=0 / instance_count=0` | ✅ 通过（证明参数从未写入） |
| `broken_export_path_draws_no_notes` | 按修复前路径（`prepare_direct` + 无 depth）渲一帧，**覆盖像素数 == 0** | ✅ 通过（**画面上一个音符都没有**） |
| `prepare_pass_populates_indirect_args` | 走 `prepare_pass`（cull）后 `vertex_count=4`、`instance_count=2` | ✅ 通过 |
| `fixed_export_path_draws_notes_deterministically` | 修复后音符出现，且同场景渲两次**逐字节相同** | ✅ 通过 |
| `prepare_pass_is_stable_across_repeats` | 连续两次 cull 的间接参数一致 | ✅ 通过 |

**根因链**（全部行号已核）：

1. 导出渲染器按 `needs_depth=false` 创建（`render_loop.rs:79-98`）⇒ `direct_pipeline = None`（`note_renderer/init.rs:195-206`）。
2. `render_pass.rs` 的兜底分支调用 `note.prepare_direct(...)`，而该函数**只写相机 uniform、不 reset indirect、不 dispatch cull**（`note_renderer/point.rs:65-83`）。
3. 绘制侧 `note.draw_direct(...)` 在无直绘管线时回退到 `draw_with_pipeline` → `draw_indirect`（`note_renderer/draw.rs:46-50, 153`）。
4. `indirect_buffer` 只在 `prepare_pass_with_pipeline` 里被写入（`note_renderer/prepare.rs:290`），导出路径从不经过 ⇒ 参数恒为创建时的全零（`init.rs:355-368`）⇒ **零图元**。

**已修**（本次提交）：

- `note_renderer.rs`：新增 `has_direct_pipeline()` 访问器，把「准备入口必须与绘制入口配套」写成可查询的事实。
- `render_pass.rs`：兜底分支按 `has_direct_pipeline()` 分流——无直绘管线时改走 `prepare_pass`（cull），否则沿用 `prepare_direct`。
- 验证：`cargo test -p lumino-gfx --lib` → **224 passed / 0 failed**；`cargo clippy -p lumino-gfx --all-targets` 无警告；`cargo fmt` 通过。

**仍待修（本轮未做，属 P0 剩余项）**：导出 pass 依然**没有 depth attachment**，因此：
- `note.wgsl` 的 `region_depth` 稳定全序仍空转（`note.wgsl:24-57`）；
- 重叠音符的赢家取决于 `cull.wgsl:96-120` 抢占式 `atomicAdd` 的输出顺序 ⇒ **理论上帧间可翻转**（上述 2 音符不重叠场景实测两次逐字节相同，但重叠场景无保障）。

**修法（二选一，推荐 A）**：

- **A. 导出 pass 恢复 depth attachment**：`ensure_textures(.., true)` + 导出渲染器用带 depth 的变体。
  - 代价：1080p `Depth32Float` 每帧 clear ≈ 8.3 MB 带宽（`docs/video_export_perf_analysis.md:237-288` 曾建议去掉，这里要**反向恢复**——因为它是像素确定性的**必要条件**，不是浪费）。
  - 收益：`region_depth` 的「区起点 + 全局源索引」是**与绘制顺序无关的全序**（`note.wgsl:31-42` 自述），恢复后 GPU 导出**天然确定性**，且与 CPU 侧的「最小索引胜」逐位同语义。
  - 用 `StoreOp::Discard` 可省掉 depth 回写带宽。
- **B. 导出走 `vs_direct`（实例序直绘）**：构建 `needs_depth=false` 的直绘管线，绘制序 = 实例序 = 确定性「后画者胜」。
  - 代价：VS 为**全量实例**各跑一遍（实测 ≈0.65 ns/实例，`params.rs:141-159`），黑乐谱 100M+ 音符直接不可用（1.5 亿外推 ≈112~130 ms/帧，`CHANGELOG.md:994`）。
  - ⇒ **B 只在「音符总量小」时可选，生产走 A。**

> ⚠ 这是一次**渲染语义变更**：重叠区赢家会从「随机」变成稳定。必须走 §6 的像素等价 harness + 真机黑乐谱夹具验收，不得「看着一样」就过（`docs/2026-10-17-数据源冗余审计.md:420` 的口径）。

#### ② 导出侧缺「主音轨」概念，与预览口径不同

`.ai-context/sessions/2026-10/2026-10-25-key-color-unify-prewarm-dedup.md:122-123` 原文：

> 视频导出仍无「主音轨」概念（`usize::MAX`）。若将来导出要与预览完全一致，需把 `current_track` 传入导出载荷。

- 影响：导出把 `track_idx=0` 当主音轨区（`note.wgsl:150-157` 用 `track == 0` 选 region），而预览用真实 `current_track`。
- 对**本需求**（CPU ↔ GPU 一致）不阻塞；但会**阻塞「成片 = 编辑器所见」**。列为待决决策 D2（§7）。

#### ③ `RenderParams` 必须由两条后端共用同一份构造

- 现状：导出侧的 `RenderParams` 由 `build_video_export_render_params` 独有构造（`render_params/build.rs:21`），`grid_instances` 恒空、`ruler_instances` 恒空、`scale_factor = 1.0`（`note_rectangle_params.rs:41-43, 62`）。
- 若 CPU 后端另写一份构造 ⇒ 第二个「CPU/GPU 可见范围公式不一致」事故（已有前科：`.ai-context/sessions/2026-06/2026-06-18-onion-cpu-gpu-range-mismatch.md:9-21`）。
- **强制约束**：CPU 后端**只接收 `RenderParams`**，不得自己从 `MidiDocument` + 配置重新推导任何 scroll/zoom/border/canvas 数值。

### 2.4 色彩口径（必须逐位复刻的量化链）

链路实测（三处独立证据互相印证）：

```
current_track_color_f32(track) → [f32;4] 线性
  → pack_key_color: (c*255.0) as u32          ← 截断，不是四舍五入
                                              crates/editor/gfx/src/note_renderer/types.rs:36-42
  → 着色器 unpack: u8 as f32 / 255.0          ← 回到「量化后的线性值」
                                              shaders/note.wgsl:103-109
  → 描边: color * 0.4（线性空间）              shaders/note.wgsl:22, 276-281
  → 写入 *Srgb 目标 → 硬件 sRGB 编码           context/create.rs:242-248（优先 is_srgb）
  → 读回字节 = sRGB 编码后的 BGRA
```

- `cpu-onionskin` 已有等价实现与**编译期断言**：`srgb_encode((0.2,0.55,1.0)) == (124,196,255)`、边框 `== (80,129,170)`（`examples/cpu-onionskin/src/render/color.rs:30-38, 203-204`），并用 `include_str!` 把 wgsl 源码头钉成事实（同文件 `:218-246`）。**这套断言直接复用为防漂移闸门。**
- ⚠ **未验证风险**：硬件 sRGB 编码是「规格允许近似」的环节（D3D11 允许查表，Vulkan 要求 IEC 61966‑2‑1）。理想公式与具体驱动的 8-bit 结果可能差 1 LSB。**必须先做 §6.1 的 256 级实测探针**再定口径。

### 2.5 栅格化规则（CPU 必须与 GPU 逐位同规则）

GPU 侧（`note.wgsl`）：

| 环节 | 规则 | 证据 |
|---|---|---|
| 顶点变换 | `x = tick*zoom.x - scroll.x + keyboard_width + canvas_offset.x`（**f32，操作数顺序敏感**） | `note.wgsl:134-140` |
| NDC | `ndc_x = x/vw*2-1`；`ndc_y = 1 - y/vh*2` | `note.wgsl:143-144` |
| 几何 | 4 顶点 → 2 个三角形（triangle-strip），**轴对齐矩形** | `note.wgsl:118-127`；`pipeline.rs:57-61` |
| 覆盖判定 | 像素中心 `(i+0.5, j+0.5)`；`cpu-onionskin` 已用 `(sx-0.5).ceil()` 表达同一规则 | `examples/cpu-onionskin/src/render/notes.rs:33-41` |
| 片元 | 纯色；边框 = `uv` 边距判定 + `×0.4`；**无 AA、无渐变** | `note.wgsl:250-281` |
| 混合 | 主音符层 `ALPHA_BLENDING`，但 `alpha == 1.0` ⇒ 等价替换写入；洋葱皮层为**不透明**管线 | `pipeline.rs:83-99`；`note_renderer/init.rs:150-159` |
| 深度 | `LessEqual` + `depth_write`；`region_depth = bitcast<f32>(region_bits + min(idx, SLOTS-1))` ⇒ **最小索引胜** | `note.wgsl:44-57`；`constants.rs:46-54` |
| 剔除 | `length > 0` 且视口矩形相交；**明确禁止 LOD 剔除亚像素音符** | `cull.wgsl:73-93`（含 `:81-82` 用户硬约束注释） |
| scissor | `(canvas_offset*scale) as u32` **截断取整** | `render_pass.rs:173-179` |

**CPU 侧对应的最小充分实现（三类写入路径，全部行块操作）**：

1. `fill_row(linear_rgba_row, x0, x1, color)` — 整行填（AVX2 `_mm256_storeu_si256`，`cpu-onionskin/src/render/simd.rs:121-141`）
2. `masked_fill_row(row, owner, ...)` — 带 z-key 比较的掩码填（AVX2 `cmpgt/movemask/maskstore/blendv`，同文件 `:143-189`）
3. 每行三段（左描边 / 填充 / 右描边）—— `cpu-onionskin/src/render/notes.rs:114-166` 已是此结构

**z-order 在 CPU 侧的等价表达**：把 `owner: u32` 从「三档掩码（0/1/2）」升级为 **`z_key: u32 = (region << 31) | global_index`**，写入条件改为 `z_key < stored_z_key`。这样与 GPU 的「最小 `region_depth` 胜」**逐位同语义**，且与绘制顺序完全无关（不需要排序，也不需要确定绘制序）。

> `cpu-onionskin` 当前的 `owner` 只有三档（`buffer.rs:3-4`）并用「同级先到先得」近似 GPU（`band.rs:4` 自述「近似」）。生产实现必须做上面这项升级，否则同轨重叠音符与 GPU 不一致。

---

## 3. 统一数据源约束的落地方式

### 3.1 既有约束原文

- `docs/2026-10-17-数据源冗余审计.md:4`：「音符数据必须以 `MidiDocument` 为唯一权威源，任何组件不得私自二次拷贝存储；GPU 资源同理。」
- 同文 `:386`：「GPU 常驻缓冲是音符实例的唯一权威，CPU 不得再持影子副本。」
- 同文 `:62` 把视频导出列为**同源正例**：「走 `Arc<MidiDocument>` 快照（`video_export_handler.rs:247-256`），同源 document。」
- `src/runner/menu/window/video_export_handler.rs:21-25`：「**不要**再起第二套音符镜像——直接消费 `MidiDocument`（唯一权威源）。」

### 3.2 本方案如何满足

| 约束 | 落地 |
|---|---|
| 禁止第二份音符镜像 | CPU 后端每帧**窗口化查询**权威数据，不构建全量实例副本。底层 API：`ChunkedList::window_range` / `iter_window`（`crates/audio/midi-model/src/chunked_list/query.rs:29-81`，两次 `partition_point` + 块级跳变）。示例侧同构写法见 `cpu-onionskin/src/render/band.rs:134-152` |
| 允许的暂存 | 每帧**窄条窗口**内的 `Vec<NoteInstance>`（数千条，帧后清空复用）；审计已把「CPU 侧暂存镜像（跨帧保留容量）」判为可接受（`docs/2026-10-17-数据源冗余审计.md:271-276`） |
| 与 GPU 后端同一 `Arc` | 沿用现有 `(*document).clone()` 块级 COW 快照 |
| 派生索引 | 长音符 underlay 表（`cpu-onionskin/src/model.rs:16, 50-71, 94-100`，约 1 MiB / 1.64 亿音符）—— 属派生索引，非音符副本 |
| ⚠ **不要直接用 `get_track_notes_in_range`** | 该 API 存在（`crates/audio/midi-model/src/document_query.rs:113-138`），但下界是固定 `tick_start - TICK_SEARCH_BUFFER`（`TICK_SEARCH_BUFFER = 19200`，`document.rs:37`）⇒ **时长超过 19200 tick 的超长音符会被漏掉**。`render_params/helpers.rs:131-141` 已把这个坑写进注释：「下界固定为 0，不使用固定 `TICK_SEARCH_BUFFER`，否则时长超过该缓冲区的超长音符在半路消失」。CPU 侧必须用「每轨实际最大音长」定下界 + 长音符 underlay 单独通道（示例 `model.rs:94-100` 的 `lookback = min(max_len, LOOKBACK_TICKS)` + `long_notes` 两层方案） |

---

## 4. ≥ 60 fps：性能论证

### 4.1 关键洞察：导出是「单调前滚 + zoom 恒定」的纯水平滚动

`note_rectangle_params.rs:30-38`：

```rust
let viewport_tick_span = (ppq * 16).max(1) as f32;          // 恒定 4 小节
let zoom_x = (rect_width - keyboard_width) / viewport_tick_span;  // 分辨率/ppq 决定，与内容无关
let scroll_x = tick as f32 * zoom_x;                        // tick 单调递增 ⇒ scroll_x 单调递增
let scroll_y = 0.0f32;                                      // 无纵向滚动
```

**每帧水平位移**：

```
px/帧 = (W - 60) × BPM / (960 × fps)
1080p / 120 BPM / 60fps →  3.88 px
1080p / 600 BPM / 60fps → 19.4  px
1080p / 999 BPM / 60fps → 32.3  px      ← 曲速上限（MIDI tempo 字段 24-bit µs/qn，实际远低于此）
```

⇒ **每帧只需光栅化一条 ≤ 40 px 宽、全高的窄条**。这正是 `cpu-onionskin` 的增量帧结构：

- 已有帧按**整数像素** `copy_within` 平移（`README.md:87-104`；`src/app/frame.rs:103-122`）
- 只对新露出的窄条跑一次 Exact 光栅化，**与全量帧同一套光栅化**（不降质量）
- 亚像素残差留在相机、窄条用**帧的实际滚动**渲染，避免累计误差（`README.md:94-96`）
- 已有逐像素回归：**`平移 + 窄条` 与同位置全量渲染 `assert_eq!(diff, 0)`**（`src/app/scroll_tests.rs`；`README.md:117-118`）

**边界情况**：`seconds_to_tick`（`video_export.rs:75-95`）在 tempo 变化处是分段线性的，`scroll_x` 仍单调 ⇒ 增量路径始终成立。仅在**首帧**需要一次全量渲染（约 25 fps 级别的成本，`README.md:89`），一次性可接受。

### 4.2 单帧预算（1080p，按实测锚点推算）

| 工序 | 数据量 | 耗时估算 | 依据 |
|---|---|---|---|
| 全帧行位移 `copy_within` | 8.29 MB | **0.7 ms** | 12 GB/s 单线程 memmove |
| 窄条光栅化（≤40px × 1080） | ≤43 k px | **0.3 ms** | `cpu-onionskin` draw p50 = 2.34 ms 覆盖全屏重画，窄条按面积比例 ≪ |
| 窄条行填 + sRGB 编码 | ≤43 k px | **0.05 ms** | 同上 |
| 键盘覆盖层 + 标尺数字重画 | ~120 k px | **0.2 ms** | 现有 `composite_keyboard` 已是逐行 `copy_from_slice`（`keyboard/texture.rs:139-155`） |
| `BufWriter<ChildStdin>` 写管道 | 8.29 MB | **0.9 ms**（8 次 1 MB write） | `.ai-context/.../2026-09-18-video-export-gpu-util-analysis.md:11-18`（实测 rawvideo 上限 237.5 fps ≈ 1.97 GB/s） |
| **小计（渲染腿）** | | **≈ 2.1 ms/帧 ⇒ ≈ 470 fps** | |

对照：`docs/video_export_perf_analysis.md:9` 曾按「每帧 8.3 MB 堆分配」算出 498 MB/s 分配压力 —— 本方案用**帧缓冲复用 + 零分配窄条**直接绕开。

### 4.3 真正的天花板是编码器，不是渲染

实测（i7-9750H 6C12T + RTX 2060 Laptop，1080p BGRA 直灌，240 帧）：

| 编码路径 | 吞吐 | 出处 |
|---|---|---|
| libx264 crf18 preset=slow（质量=**高**） | **36.9 fps** ❌ | 同上 `:11-18` |
| libx264 crf23 preset=medium（质量=**中，当前默认**） | **62.6 fps** ⚠ 勉强 | 同上 |
| libx264 crf28 preset=veryfast（质量=**低**） | **89.5 fps** ✅ | 同上 |
| h264_nvenc cq23 p4 | **112.6 fps** ✅ | 同上 |
| hevc_nvenc cq23 p4 | **117.4 fps** ✅ | 同上 |
| 纯 rawvideo 读取上限 | **237.5 fps（1.97 GB/s）** | 同上 |

**结论与动作**：

1. 渲染腿（≈2.1 ms）**不是瓶颈**，去掉 GPU 读回后比现状更快（现状 `drain=7.5ms`，见 §4.4）。
2. 「≥60fps」的达标前提是**编码后端选择**：
   - 硬件后端（NVENC / VideoToolbox / QSV / AMF）→ 稳过；
   - 纯软件：`preset` 必须 ≥ `veryfast`（62.6 → 89.5）；`QualityPreset::High`（slow）**在 60fps 目标下必须显式告警**。
3. 4K 时 raw BGRA 管道 33.2 MB/帧 / 1.97 GB/s ⇒ **架构上限 ≈ 60 fps**（同文档 `:20-22`）——4K@60fps 的 CPU 后端需先解决「送进管道的字节数」，不在本方案一期范围。

### 4.4 与现状（GPU 后端）的对比

`examples/miditrail_export_bench.rs` 真机整链路（19.4M 音符，1080p60，NVENC，`.ai-context/sessions/2026-09/2026-09-18-...:34-39`）：

| 路径 | 整链路 fps | 分段 |
|---|---|---|
| driven | **45.7 fps** | `render=1.0ms drain=7.5ms encode=4.2ms` |
| legacy | **26.2 fps** | `render=15.4ms` |

⇒ 现状瓶颈明确是 **`drain`（GPU→CPU 读回 7.5 ms）**。CPU 后端**结构上不存在这一项**，这也是「消除 7.5 ms」的直接收益。

---

## 5. 实现方案

### 5.1 模块划分与依赖边界

**不新建 crate**。以已存在的 `crates/editor/cpu-onion` 为内核，新增「整帧导出」适配层（同 crate 内新增子模块，保持其「不依赖 gfx/wgpu」的干净依赖图）：

```
crates/editor/cpu-onion/src/
  ── 已有，直接复用 ──────────────────────────────────────────────
  simd.rs / buffer.rs / slot.rs      行填充 + owner 掩码填（需扩 z_key）
  band.rs / notes.rs / compose.rs    音符光栅 + band 合成
  camera.rs                          CanvasCamera（纯 canvas 坐标）
  color.rs                           NoteStyle（调色板 / ×0.4 / sRGB）
  index.rs                           TrackIndex（长音符 / lookback）
  density/                           密度金字塔 + 彩色覆盖 LOD
  renderer.rs + renderer/{strip,region,output}.rs   渲染器与增量调度
  ── 本方案新增（导出后端）──────────────────────────────────────
  src/export/mod.rs       ExportRenderer：整帧导出入口（消费 RenderParams）
  src/export/params.rs    RenderParams → CanvasCamera / 几何口径映射（含 keyboard/ruler 偏移）
  src/export/background.rs 不透明背景 + 可分离网格（infinite_grid.wgsl 等价）
  src/export/ruler.rs     ruler.wgsl 等价（实例化 quad + alpha 混合）
  src/export/main_track.rs 主音轨层（crate 现状只画洋葱皮）
  src/export/zkey.rs      z_key = (region<<31)|index 全序（取代三档 owner）
  src/export/wire.rs      与 app 导出管线对接的稳定 API（BGRA 字节出口）
  tests/export_parity.rs  与 GPU 后端的差分测试（dev-dep lumino-gfx）
```

**依赖边界**：`cpu-onion` 现状依赖 `lumino-midi-model` + `lumino-extras` + `tokio`（`Cargo.toml:14-20`），**不依赖 gfx/wgpu** —— 这个性质必须保住，导出层不得引入 wgpu。`NoteInstance` / `pack_key_color` / `calculate_border_width` / `is_black_key` / `CameraUniform` 目前在 gfx（`note_renderer/types.rs:24-120`）：
- **一期**：导出层在 `cpu-onion` 内定义等价的 POD（`ExportNote` / `z_key` / 颜色打包），并用**编译期常量断言**钉住与 wgsl/gfx 的一致性（范式已有：`cpu-onion/src/color.rs` 与 `utilities` 的 `style_constants.rs` 测试）；
- **后续可选**：把这些 POD 抽到 `lumino-midi-model` 或新的 `lumino-render-common`，让 gfx 反向复用（会动 gfx 公共 API，不在本方案一期）。

### 5.2 对外 API（草案，扩 `CpuOnionRenderer` 而非另起类型）

```rust
// crates/editor/cpu-onion/src/export/mod.rs
/// 一帧导出的渲染输入 —— 几何全部来自 `RenderParams`，本层不自行推导。
pub struct ExportFrameInput<'a> {
    pub width: u32,
    pub height: u32,
    pub scroll_x: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub max_key_index: f32,
    pub keyboard_width: f32,
    pub ruler_height: f32,
    pub canvas_offset: (f32, f32),
    pub canvas_size: (f32, f32),
    pub background: [f32; 4],          // 线性背景色（LoadOp::Clear 同值）
    pub grid_colors: GridColors,       // color_bg / black_key / bar / beat / half / grid / key_line
    pub time_signatures: &'a [(u32, u8, u8)],
    pub ppq: u32,
    /// 权威数据源（唯一，不拷贝）。
    pub document: &'a MidiDocument,
    pub index: &'a TrackIndex,
    /// 导出语义：None ⇒ 全部音轨都按「非主轨」区域绘制（与当前 GPU 导出 `track==0` 规则对齐）。
    pub current_track: Option<usize>,
    pub muted: &'a [bool],
}

pub struct ExportRenderer { /* 复用 CpuOnionRenderer 的 band/simd/缓冲 + 新增 bg/grid/ruler */ }

impl ExportRenderer {
    pub fn new(width: u32, height: u32, worker_threads: usize, track_count: usize) -> Self;

    /// 渲染一帧并写出可直接灌 ffmpeg 的 BGRA 字节（`out` 复用，零分配）。
    /// 内部按 `scroll_x` 增量决定「整帧重画」还是「整数像素平移 + 窄条」。
    pub fn render_frame(&mut self, input: &ExportFrameInput<'_>, out: &mut Vec<u8>) -> ExportFrameStats;
}

pub struct ExportFrameStats {
    pub shifted_px: u32,        // 0 = 整帧重画
    pub strip_width: u32,
    pub notes_rasterized: u64,
    pub raster_us: u64,
    pub shift_us: u64,
    pub encode_us: u64,         // 线性 → sRGB → BGRA
}
```

### 5.3 接线点（最小侵入，沿用 `MidiConsole` 双后端形态）

| 位置 | 改动 |
|---|---|
| `crates/core/message/src/events/window/video.rs` | 新增 `VideoRenderBackend { Gpu, Cpu }`，加到导出配置载荷 |
| UI 导出对话框 | 新增下拉「渲染后端：GPU / CPU（无显卡依赖）」；默认 `Gpu` |
| `video_export_handler.rs` | 新增分支：`backend == Cpu` 时走 CPU 路径（不发 `RenderVideoFrame` 命令、不建 staging ring、不 `wait_for_frame`） |
| `memory_task/enqueue.rs` | CPU 后端**直接**在 `video-render` 线程内渲染，把 `Vec<u8>` 送 `frame_tx`（复用现有 `frame_tx_waterfall` 通道与背压） |
| `composite.rs` | 不改：CPU 后端同样需要 `composite_keyboard` + `composite_ruler_numbers` |
| `pipeline.rs` | 不改：预填充/取消/统计/log 全部复用 |
| `FfmpegEncoder` | 不改：`input_pix_fmt = "bgra"` 一致 |
| `gfx` | 仅 §2.3-① 的 depth 恢复（若采用方案 A） |

**收益**：帧通道、背压、取消、进度、错误上报、ffmpeg 直灌、对象池**全部零改动复用**，新增代码集中在 `lumino-cpu-onion` 的导出适配层。

### 5.4 每帧渲染顺序（必须与 `render_pass.rs:154-274` 逐条对齐）

```
if 需要整帧重画（首帧 / 尺寸变化 / scroll 回退）:
    1. 全帧 Clear(background_color)                      ← LoadOp::Clear
    2. grid::render(screen_x 层, screen_y 层)            ← 可分离，O(W+H)
    3. onion::render(window 内的洋葱皮音符, z_key 区域=ONION)
    4. note::render (window 内的主轨音符, z_key 区域=MAIN)
    5. ruler::render(aligned 实例)
else:
    frame_rows[..].copy_within(offset = -dx*4)           ← 整数像素
    strip_x = [W - dx, W)
    对 strip 逐行: Clear → grid → onion → note（z_key 只在 strip 缓冲内）
    键盘区 + 标尺区每帧重画（不随内容滚动）
最终: 线性 f32 → sRGB 编码 → BGRA 字节 → out
```

> 注意：z 缓冲（`z_key`）**只需 strip 宽度**（`strip_w × height`），不必全帧 —— 已定稿像素永不再参与光栅化。这把额外内存从 8.3 MB 降到 <0.2 MB。

### 5.5 对账：`lumino-cpu-onion` 覆盖了什么、还缺什么

以已存在的 crate 为基线（依据 `crates/editor/cpu-onion/src/lib.rs:1-35` 与各模块实测）：

| 能力 | 状态 | 依据 |
|---|---|---|
| AVX2 `fill_row` / `masked_fill_row` + 标量兜底 | ✅ **已生产化** | `src/simd.rs`（317 行） |
| 多 band 并行（tokio） + band 缓冲槽复用 | ✅ 已生产化 | `src/renderer.rs:49-104`；`src/band.rs` |
| 增量窄条 `render_strip` + 整数像素平移 | ✅ **已实现**（Host 尚未接线） | `src/renderer/strip.rs:1-84` |
| 脏区重绘 `render_region` | ✅ 已实现 | `src/renderer/region.rs` |
| AABB + 像素中心覆盖 + 每侧 2px 描边 | ✅ 已生产化 | `src/notes.rs`（330 行） |
| 调色板 / `×0.4` 描边 / sRGB 编码 | ✅ 已生产化 + 常量断言 | `src/color.rs`；`tests/style_constants.rs` |
| 长音符索引 / lookback | ✅ 已生产化 | `src/index.rs`（354 行）；`LOOKBACK_TICKS` |
| 密度金字塔 + 彩色覆盖 LOD | ✅ 已生产化（导出用不上，`zoom_x` 恒定） | `src/density/` |
| 1080p 性能基准 | ✅ 已有 | `tests/perf_1080p.rs`、`tests/zoom_bench.rs` |
| **主音轨层** | ❌ **缺**（crate 明确只画非当前轨） | `lib.rs:4` |
| **不透明背景 + 背景色** | ❌ **缺**（输出透明 alpha=0） | `lib.rs:5`；`frame.rs:9-10` |
| **网格层（`infinite_grid.wgsl` 等价）** | ❌ **缺**。示例的 `chrome.rs` 是自有暗色主题：硬编码颜色、固定 `ppq×4`、无拍号、无 LOD 分层、无 6 档细分 —— **不能当基线** | `examples/cpu-onionskin/src/render/chrome.rs:14-25, 87-100` |
| **标尺层（`ruler.wgsl` 等价）** | ❌ 缺（导出需要实例化 quad + `tick_type` 三档亮度 + alpha 混合） | `ruler.wgsl:64-84` |
| **`RenderParams` 几何口径** | ❌ 缺（crate 用 `CanvasCamera`，不含 `keyboard_width` / `ruler_height`） | `lib.rs:18-22`；`camera.rs` |
| **z 全序（`region_depth` 等价）** | ⚠ **需升级**：现状是「first-wins + 三档 owner」，而 GPU 是「区内按全局索引严格全序」 | 需按 §2.5 改为 `z_key` 比较 |
| 键盘栏 / 小节号数字 | ✅ **已在 app 侧实现**（CPU） | `video_export/keyboard/texture.rs`；`video_export.rs:137-209` |

⇒ **剩余工作量 = 主音轨层 + 不透明背景 + 网格 + 标尺 + `RenderParams` 适配 + z_key 升级 + 接线**。比「从零造」小得多，但**网格与标尺仍是净新增**，不能算成「搬代码」。这也是 §7 把 P2 单列的原因。

---

## 6. 验收与测试方案（像素级一致必须可执行）

### 6.0 验收门槛（D3 已定 2026-10-31：可量化的「视觉一致」，非逐字节）

用户拍板：**「样子完全一样就行，不需要超级严格」**。但仓库明文禁止把「看着一样」当验收（`docs/2026-10-17-数据源冗余审计.md:420`：「不能靠『看着一样』」），所以把主观口径落成**三条机器可判**的门槛：

| # | 门槛 | 阈值 | 判据 |
|---|---|---|---|
| G1 | **逐像素差** | 单通道差 ≤ **2** LSB 的像素占比 ≥ **99.5%**；不存在单通道差 > **16** 的像素 | 整帧扫，不抽样 |
| G2 | **覆盖一致** | `|covered_cpu − covered_gpu| / max(covered_gpu,1)` ≤ **0.2%** | 避免整块音符缺失/多画 |
| G3 | **结构一致**（不可被 G1/G2 平均掉的部分） | ①每条音轨的代表像素颜色取自同一调色板项 ②重叠探针点的赢家音轨 ID 相同 ③描边像素数与填充像素数比例一致（±1%） | 定向探针，不是随机采样 |

这三条同时满足 = 「样子完全一样」。任何一条失败都必须在报告里定位到具体音符/像素，不许含糊。

**不做的事**（明确排除，避免过度工程）：不追 CPU↔GPU 逐字节相等；不追硬件 sRGB 查表与理想公式的 0/1 LSB 差异；不追三角形对角线 UV 插值的 ULP 级差异。这些都被 G1 的 2 LSB 容差吸收。

### 6.1 sRGB 探针（已降级为可选信息项）

D3 定为宽松口径后，本探针不再是闸门，只在需要解释「为什么差 1~2 LSB」时提供数据：

1. 写一个 debug shader：`fs_main` 返回 `vec4(i as f32 / 255.0, 0, 0, 0)`，`i = 0..255`；
2. 渲染到 `Rgba8UnormSrgb`，读回 256 个字节；
3. 与 `srgb_encode_round_to_nearest(i/255.0)` 逐字节比对，记录差异分布。

- 全等 → G1 的 2 LSB 余量纯属富余；
- 差 ≤1 LSB 且模式稳定 → 符合预期，G1 容差正好覆盖；
- 差 >2 LSB 或随驱动变化 → **必须**改为渲到 `Rgba8Unorm` + 在代码里显式做 sRGB 编码（把不确定性从硬件搬到代码）。

### 6.2 差分测试（核心闸门，判据 = §6.0 的 G1/G2/G3）

放 `crates/editor/cpu-onion/tests/export_parity.rs`（dev-dep `lumino-gfx`）：

```rust
#[test]
fn cpu_matches_gpu_within_visual_gate() {
    for case in corpus() {                        // 场景语料见 §6.3
        let gpu = render_on_gpu(&case);           // 复用 direct_tests.rs 的 render()+read_pixels()
        let cpu = render_on_cpu(&case);
        assert!(g1_max_channel_delta(&cpu, &gpu) <= 2, "{}：单通道差超 2 LSB", case.name);
        assert!(g1_ratio_within(&cpu, &gpu, 2, 0.995), "{}：差 ≤2 LSB 的像素占比不足 99.5%", case.name);
        assert!(g2_coverage_delta(&cpu, &gpu) <= 0.002, "{}：覆盖像素数偏差超 0.2%", case.name);
        g3_structural_asserts(&cpu, &gpu, &case);  // 调色板项 / 重叠赢家 / 描边比例
        assert!(covered(&gpu) > 0, "{}：GPU 侧必须真的画出东西（反假绿）", case.name);
    }
}
```

**既有范式直接复用**：

| 范式 | 出处 | 复用点 |
|---|---|---|
| 整缓冲逐字节相等（不抽样） | `crates/editor/gfx/src/note_renderer/direct_tests.rs:254-291` | 断言形态（D3 放宽后仅用于「同后端确定性」） |
| ±1 LSB + 差异率阈值 | `.../driven_equivalence/pixels.rs:61-65,124,140-143` | G1 容差来源 |
| 反假绿守卫（必须真画出东西） | `direct_tests.rs:285-290` | 防「两边都空」 |
| 连续 ≥64 帧逐位一致 | `note_renderer/depth_tests/gpu_common.rs:7-8` | 确定性验证 |
| 增量 vs 全量逐像素 | `crates/editor/cpu-onion/src/renderer/strip.rs` + `tests/region_render.rs` | 窄条等价 |
| 用 wgsl 源码头做常量断言 | `crates/editor/cpu-onion/tests/style_constants.rs` | 防漂移 |
| **导出路径 cull 契约（已落地）** | `crates/editor/gfx/src/note_renderer/export_path_tests.rs` | 5 条：间接参数 / 空帧取证 / 修复后确定性 |

### 6.3 场景语料（缺一不可）

| # | 场景 | 覆盖的风险 |
|---|---|---|
| 1 | 同 key 三音重叠（不同 start/length） | z-order `region_depth` 语义（R4 ULP 翻转） |
| 2 | 跨音轨重叠（主轨 vs 洋葱皮） | 区域优先级 |
| 3 | 静音轨洋葱皮 | 剔除口径 |
| 4 | 亚像素宽音符（`zoom_x×length < 1px`） | 「禁止 LOD 剔除」硬约束 |
| 5 | 1px 边框 / 5px 边框（`calculate_border_width` 上下界） | 描边内缩取整 |
| 6 | 黑键 / 白键 / key 边界 | 背景色与琴键线 |
| 7 | 拍号中途变化（3/4 → 6/8） | 网格 LOD 与拍号段 |
| 8 | tempo 剧烈变化 | tick→scroll 分段 |
| 9 | 视口边缘裁剪（音符半出屏） | scissor 截断 |
| 10 | 空文档 / 单音符 | 边界 |
| 11 | 密集死历史（100 万音符但只有少量可见） | 「等价集必须含密集死历史」教训（`.ai-context/sessions/2026-09/2026-09-05-cull-closed-loop.md:4-6`） |
| 12 | 连续 64 帧滚动 | 增量路径 + 确定性 |

> ⚠ **夹具缺口**：黑乐谱 `.mid` 夹具被 `.gitignore` 排除（`docs/2026-10-17-数据源冗余审计.md:420-422`）。合成语料可进 CI，真实夹具需本地/夜间跑 —— **这是既有记录在案的空白，不是本方案引入的**。

### 6.4 性能门（CI 可跑）

`crates/editor/cpu-onion/tests/export_throughput.rs`（或 `examples/cpu_export_bench.rs`，形态照抄 `examples/miditrail_export_bench.rs`）：

```
门槛（1080p，1080 帧）：
  渲染腿 p99 ≤ 4.0 ms          （预留 4x 余量给慢机器）
  整链路 wall fps ≥ 60          （编码后端按 §4.3 选取）
  p99 帧时 ≤ 16.67 ms
  零大对象分配：render_frame 内分配次数 == 0（容量预热后）
```

---

## 7. 分阶段实施计划

| 阶段 | 内容 | 产出 | 验收 | 估时 |
|---|---|---|---|---|
| **P0** | ①**已修**：导出主音符层 cull 缺失（`has_direct_pipeline()` 分流）②sRGB 探针**降级为可选**（口径已放宽，见 D3）③待办：导出 pass 恢复 depth（关掉重叠序依赖 cull 顺序）④待办：`current_track` 待决 | 探针测试 5 条 + 修复 | **已达成**：真机取证「修复前覆盖像素 0 / 修复后画出且两次逐字节相同」；`gfx` 224 测试全绿 | 已完成一半 |
| **P1** | 在 `cpu-onion` 内加 `export/` 层：`RenderParams` → 几何适配 + **不透明背景** + **主音轨层**；z 全序升级为 `z_key` | 能渲出「背景 + 洋葱皮 + 主音轨」的整帧（无网格/标尺） | 与 GPU 差分测试场景 1/2/3/4/5/10 全绿 | 2 d |
| **P2** | `export/background.rs`（可分离网格）+ `export/ruler.rs`（实例化 quad + alpha 混合） | 整帧视觉完整（含网格与标尺） | 差分测试 6/7/9 全绿 | 1.5 d |
| **P3** | 接线 `renderer/strip.rs` 的增量路径（crate 已有但 Host 未用）+ BGRA 输出出口 | 单调前滚零全量重画 | 差分测试 12 全绿；性能门 1080p 单帧 ≤4 ms | 2 d |
| **P4** | 接线：配置枚举 + 对话框下拉 + `video_export_handler` 分支；bench | 用户可选 CPU 后端 | 端到端 1080p ≥60 fps；成片与 GPU 后端视觉一致 | 1 d |
| **P5（可选）** | `examples/cpu-onionskin` 改为消费 `lumino-cpu-onion`（消重复实现），或补「成片」层 | 示例删重复光栅代码 | 示例基准不回退 | 0.5 d |

**关键路径**：P0-② 是硬前置 —— 不修 GPU 确定性，P1 的差分测试**必然随机失败**。

---

## 8. 风险与待决决策

### 8.1 风险

| # | 风险 | 影响 | 缓解 |
|---|---|---|---|
| R1 | 硬件 sRGB 编码与理想公式差 1 LSB | 逐字节断言失败 | §6.1 探针先做；兜底改 `Rgba8Unorm` + 显式编码 |
| R2 | GPU 光栅化 8-bit 子像素量化细节与 CPU `ceil(v-0.5)` 不完全等价（尤其 `<1px` 宽音符与边框临界） | 边缘像素差 1 | 差分测试逐例定位 → 按实测结果校准 CPU 取整式，而不是按「理论应该」；必要时把临界宽度规则写进共享常量 |
| R3 | 三角形对角线导致的 UV 插值边界差 | 描边/填充交界 1px 差 | 矩形轴对齐时对角线两侧 UV 一致；用场景 5 专门覆盖 |
| R4 | 恢复 depth 会改变现有 GPU 成片外观（重叠赢家从随机变确定） | 用户可见变化 | 变更本身是**修复**；CHANGELOG + 对话框说明；只在导出 pass 生效，不影响编辑器实时渲染 |
| R5 | 大 tempo（>600 BPM）窄条变宽，回退全量 | 个别段落掉到 25 fps | 窄条宽度上限自适应；超阈值自动整帧重画并打点 |
| R6 | 4K 时管道带宽架构上限 ≈60 fps | 4K@60 不可保证 | 明确标注 4K 不在本方案一期指标；后续走 ffmpeg 侧 YUV 或直方图（`docs/video_export_perf_analysis.md:458-462` 已列） |
| R7 | `lumino-gfx` 依赖把 wgpu 带进「CPU 后端」 | 概念上的纯度损失（实际无功能影响，App 本来就链 wgpu） | 后续抽 `lumino-render-common`；一期文档标注 |

### 8.2 待决决策（需产品/架构拍板）

| # | 决策 | 选项 | 建议 |
|---|---|---|---|
| **D1** | GPU 导出确定性的修法 | A 恢复 depth（成本 8.3 MB/帧带宽，语义=REND-001）vs B `vs_direct` 直绘（零 depth 但 O(N) VS 不可用） | **A** |
| **D2** | 导出是否引入「主音轨」概念（与预览一致） | 是（传 `current_track`）vs 否（沿用 track==0） | **是**，否则「成片 = 编辑器所见」永远不成立；但可放 P5 |
| **D3** | 「像素级一致」口径 | (a) CPU↔GPU 逐字节 (b) 可量化的「视觉一致」(c) 仅跨帧确定 | **已定（用户 2026-10-31）：走 (b)「样子完全一样」**。⚠ 但「看着一样」不可作为验收（`docs/2026-10-17-数据源冗余审计.md:420` 明令），故落成可测量门槛 → 见 §6.2。逐字节 (a) 不作为目标，`sRGB 探针`随之从「闸门」降级为「可选信息项」 |
| **D4** | 60fps 是否绑定编码后端 | 是（硬件加速为达标前提）vs 否（软件 veryfast 为下限） | 门槛按 **(软件 veryfast ≥ 89.5 fps) 判定渲染+管道合格**，硬件后端另测 |

---

## 9. 附录 A：与 `docs/video_export_perf_analysis.md`（2026-07-19）的对账

该报告 7 项瓶颈的现状（本次复核）：

| 原编号 | 原结论 | 现状 | 证据 |
|---|---|---|---|
| P0-1.1 每帧新建 `Vec<u8>` | 5~10x | ✅ **已修**：帧对象池 `frame_pool` | `export_pipeline/staging.rs:30-31, 300-308, 340-354` |
| P0-1.2 三线程串行 | 2~3x | ⚠ **部分**：已改为直连写入 + `PIPELINE_DEPTH=4` 预填充；仍存一个 `FrameSender` 中转 | `ffmpeg.rs:26-30`；`pipeline.rs:79` |
| P0-1.3 逐行去 padding | 1.5~2x | ✅ **已修**：无 padding 走整块 `copy_nonoverlapping` fast path | `staging.rs:310-330` |
| P1-2.1 无用 depth 纹理 | 1.2x | ✅ **已修**：导出 `ensure_textures(.., false)` | `crates/editor/gfx/src/render_thread/render_loop/runner/video_export.rs:111`；`render_loop.rs:79-98` |
| P1-2.2 每帧 `create_view` | 1.1x | ✅ **已缓存** | `render_pass.rs:25-38, 276-279` |
| P1-2.3 走通用渲染路径 | 1.2x | ⚠ **未修**（仍 `prepare_renderers` 全量） | `video_export.rs:175` |
| P1-2.4 depth `StoreOp::Store` | 1.1x | ✅ 已改 `Discard`（导出无 depth 后自然无此项） | `render_pass.rs:47-54` |
| Bug 4.1 `pix_fmt` 与纹理格式不匹配 | — | ✅ **已修**：按 surface 格式推导 `input_pix_fmt` | `video_export_handler.rs:120-126` |

**注意**：本次方案要**反向重开** P1-2.1（恢复 depth）—— 因为该报告的优化目标（带宽）与像素确定性冲突，且当时并不存在「CPU 等价」需求。

---

## 10. 附录 B：证据索引（速查）

| 主题 | 位置 |
|---|---|
| 导出入口 / 可重入守卫 / 线程 | `src/runner/menu/window/video_export_handler.rs:28-47, 172-174` |
| 单一权威源注释 | `src/runner/menu/window/video_export_handler.rs:21-25` |
| 帧管道（预填充/取消/统计） | `src/runner/menu/window/video_export_handler/pipeline.rs:74-243` |
| 单帧入队 / CPU 模式分支 | `src/runner/menu/window/video_export_handler/memory_task/enqueue.rs:10-292` |
| 合成 + 编码 | `src/runner/menu/window/video_export_handler/composite.rs:41-154` |
| tick↔秒（统一入口） | `src/runner/menu/window/video_export.rs:47-126` |
| 键盘贴图 / 合成 | `src/runner/menu/window/video_export/keyboard/texture.rs:13-179` |
| 排序 / 打包 / 全量收集 | `src/runner/menu/window/video_export/render_params/helpers.rs:17-115` |
| 导出 `RenderParams` 构造 | `src/runner/menu/window/video_export/render_params/note_rectangle_params.rs:11-76` |
| 导出渲染分支（无 depth） | `crates/editor/gfx/src/render_thread/render_loop/runner/video_export.rs:43-229` |
| 导出专用渲染器集合 | `crates/editor/gfx/src/render_thread/render_loop.rs:74-98` |
| 无 depth ⇒ 无直绘管线 | `crates/editor/gfx/src/note_renderer/init.rs:189-206`；`draw.rs:40-70` |
| cull 抢占序（不确定性根因） | `crates/editor/gfx/src/shaders/cull.wgsl:96-120` |
| 稳定深度语义 | `crates/editor/gfx/src/shaders/note.wgsl:24-57, 146-157, 224-233` |
| 网格 shader（可分离） | `crates/editor/gfx/src/shaders/infinite_grid.wgsl:191-295` |
| 标尺 shader | `crates/editor/gfx/src/shaders/ruler.wgsl:64-84` |
| sRGB target 选择 | `crates/editor/gfx/src/context/create.rs:240-248` |
| 读回 ring / 对象池 | `crates/editor/gfx/src/render_thread/export_pipeline.rs:75-128`；`.../staging.rs:24-355` |
| ffmpeg 直灌 | `crates/tools/export/src/video/ffmpeg.rs:55-175`；`.../ffmpeg/args.rs` |
| CPU 光栅参考实现 | `examples/cpu-onionskin/src/render/{notes,simd,band,buffer,color,chrome}.rs` |
| CPU 光栅实测 | `examples/cpu-onionskin/README.md:41-62, 87-104, 193-202` |
| 增量 vs 全量逐像素回归 | `examples/cpu-onionskin/src/app/scroll_tests.rs` |
| 像素等价 harness 范式 | `crates/editor/gfx/src/note_renderer/direct_tests.rs:254-314` |
| ±1 LSB 容差口径 | `crates/editor/gfx/src/miditrail_renderer/tests/driven_equivalence/pixels.rs:61-65` |
| 编码器吞吐实测 | `.ai-context/sessions/2026-09/2026-09-18-video-export-gpu-util-analysis.md:11-22, 34-46` |
| 单一权威源审计 | `docs/2026-10-17-数据源冗余审计.md:4, 62, 267, 386, 420-422, 968` |
| GPU 导出优化未闭环项 | `docs/video_export_perf_analysis.md:5-16, 326-371` |
| 导出缺主音轨 | `.ai-context/sessions/2026-10/2026-10-25-key-color-unify-prewarm-dedup.md:122-123` |
| CPU/GPU 可见范围事故前科 | `.ai-context/sessions/2026-06/2026-06-18-onion-cpu-gpu-range-mismatch.md:9-21` |
| 「稀疏 harness 测不出密集死历史」 | `.ai-context/sessions/2026-09/2026-09-05-cull-closed-loop.md:4-6` |

---

## 11. 附录 C：档案对账（本方案与既有决策的关系）

| 项 | 档案现状 | 本方案的处置 |
|---|---|---|
| 「统一数据源 `MidiDocument`」 | 有正式口径（`docs/2026-10-17-数据源冗余审计.md:4,62,386,968`），但**未进入 `AGENTS.md`** | §3 明确满足；建议顺手补进 `AGENTS.md`（下一条待办） |
| 视频导出的任务卡 | **一张 PREF/EXP/REND 编号卡都没有**（卡片全在音频域） | 建议本方案落地时按 PREF 语境开卡，把 §6 的验收与 §7 的门槛写进卡面 |
| 「像素级一致」验收范式 | 已有 5 类成熟范式（§6.2 表格），但**从未覆盖「导出成片」** | §6.2 直接复用范式，首次把覆盖面扩到导出 |
| 「导出 ≥60fps」 | **从未被写成目标**；现有 60fps 数字都是预算推算或抱怨 | §4.3 把「编码器选择」写成显式达标前提（D4） |
| 「CPU 光栅等价」方案 | 术语与方案**在仓库中不存在**；仅 `examples/cpu-onionskin` 一个示例，且未被 `docs/`、`CHANGELOG.md` 引用 | 本文即首次给出方案定义 + 生产化路径（§5） |
| `.ai-context/decisions` / `patterns` / `summaries` | 三个目录 **0 文件** | 本方案落地时把 §8.2 的 D1~D4 结论写入 `decisions/` |
| 底色 / 色彩空间决策 | 导出域 `yuv420p`/`色彩空间`/`gamma` **0 命中**，只用 `bgra`/`rgba` | §2.4 首次把量化链写成可执行口径；待 D3 定案 |
| 黑乐谱 `.mid` 夹具 | 被 `.gitignore` 排除（`审计:420-422`） | §6.3 合成语料进 CI，真实夹具夜间/本地，风险已显式登记 |
