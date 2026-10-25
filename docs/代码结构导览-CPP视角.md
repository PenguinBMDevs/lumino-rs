# Lumino 代码结构详解（C++ 视角）

面向只懂 C++ 的读者。先讲构建模型与分层，再逐 crate 深入到关键文件、关键类型、数据流与线程模型。语法疑问查文末速查表。

## 1. 项目与构建模型

Lumino：Black MIDI 编辑器。技术栈：`winit`（窗口/事件循环）+ `iced`（Elm 架构 UI 框架）+ `wgpu`（Vulkan/D3D12/Metal 封装层）+ `tokio`（异步运行时，仅网络/云）+ XSynth/KDMAPI/GPU 合成（音频）。

| Rust | C++ 对应物 |
|---|---|
| Workspace（根 `Cargo.toml`） | CMake 超级构建 |
| crate | 静态库目标 + 命名空间 |
| `Cargo.lock` | 依赖版本锁定（conan lock） |
| `[patch.crates-io]` | 用 git fork / 本地目录覆盖第三方（FetchContent Override） |
| `vendor/iced_wgpu` | vendored 补丁版第三方（菜单悬停文字闪烁修复） |
| `build.rs` | 构建脚本（Windows 子系统标记、图标资源嵌入） |
| `features`（`tracy`/`gpu-reconnect-test`） | CMake option |
| profile `dev`/`release`/`fast-release` | Debug / Release / RelWithDebInfo |

注意 `Cargo.toml` 中：debug 构建对 xsynth-* 合成器与 export/loader 强制 `opt-level=3`（实时音频在 opt-0 下慢 10 倍）；`panic="unwind"` 是硬约束（GPU 设备丢失的 `catch_unwind` 防护依赖它）。

## 2. 总体分层

```
第0层（零依赖基础）  core · diagnostics · extras
第1层（数据模型）    midi-model · note-core · cloud · project
第2层（解析/状态）   midi-loader · message · editor-state
第3层（IO/渲染/导出）midi-io · midiplayer · gfx · export · ui-core · collaboration · gpu-synth
第4层（界面部件）    ui-editor · ui-settings · dialog
第5层（UI 汇总）     ui  ←—— src/ 可执行壳层
```

主线程即 UI 线程。后台线程（渲染、音频、播放、云、协作）一律通过消息通道回到主线程，从不直接碰 UI。

## 3. 数据权威源（最重要的三条数据链）

理解本项目先记住谁拥有数据：

1. **音符数据唯一权威 = `MidiDocument`**（lumino-midi-model）。编辑器、播放器、导出器都从它读。
2. **GPU 侧音符唯一权威 = GPU 字节**（gfx 的 `GpuNoteBuffer`），CPU 不维护影子副本，增量更新直接在 GPU buffer 上搬移。
3. **撤销历史 = 块级 COW 快照**：`Arc<ChunkedList<NoteEvent>>` 共享数据块，快照只拷指针。

```
MIDI 文件 --midi-loader--> MidiDocument --note-core(History/空间索引)--> 编辑器
                                |                    |
                                v                    v
                     midi-io(播放→合成器)      gfx(渲染线程→GPU buffer)
                                |                    |
                                v                    v
                     export(音频/视频离线渲染)   Surface present
```

## 4. src/ 可执行壳层

| 文件/目录 | 职责 |
|---|---|
| `main.rs` | 入口：GPU 探针子进程分支（必须最先）→ 日志 → puffin → OOM 监控 → 事件循环 → `run_app(&mut runner)` |
| `runner.rs` + `runner/` | winit `ApplicationHandler`：窗口/MIDI 文件/云操作/菜单/GPU 重连 |
| `services.rs` + `services/` | `FileService`（后台文件 IO）、`CollaborationService`（带连接 token 防旧连接复活） |
| `device_probe.rs` + `device_probe/` | GPU 兼容检测的子进程隔离 |
| `storage.rs` + `storage/` | config.json / ui_state.json 持久化 |
| `cli.rs` / `logging.rs` | clap 命令行；tracing + 异步文件日志（10 份轮转 + panic hook） |

关键类型：
- `enum UserEvent`（winit 事件循环泛型参数，≈ 自定义窗口消息）：`UiWake`（任意后台线程唤醒 UI）、`GpuDeviceLost{reason,message}`（编辑器 wgpu 设备丢失）、`LgsDeviceLost`（GPU 合成器**独立**设备丢失——两者零共享 device）。
- `struct RunnerInner`：聚合 `WindowState`/`MidiState`/`FileState`/`CollabState`/`TestState` + `cloud: Arc<Mutex<CloudManager>>` + `cloud_status: Arc<RwLock<快照>>`（UI 只读快照，永不等网络锁）+ `pending_close_action`（保存确认框挂起时的动作）。

### GPU 重连状态机（REND-019，逐帧推进、绝不阻塞 UI）

1. wgpu 回调线程（任意线程）→ `EventLoopProxy.send_event(GpuDeviceLost)`（无锁投递，回调线程只做原子置位）。
2. 主循环创建 `ReconnectSession`；此后 `about_to_wait` 每帧调 `step()`，每帧最多推进一步：`StopRenderThread → RecreateDevice`（专用线程发起，后续帧 try_recv 收割）`→ RebuildInfra → ReplayData → RestartRenderThread → Recovered/Failed`。
3. 失败退避 1s 重试，上限 3 次；进 `Failed` 前必须完成自动保存。依赖注入 `trait ReconnectOps<D>` 便于测试 Mock。

### device_probe：为什么 GPU 检测要子进程

进程内检测线程若卡死在驱动 FFI 里，Rust 没有线程取消，线程 detach 后**永久持有驱动锁**，拖死主渲染。只有子进程能被强杀。协议：子进程写 JSON `ProbeReport{version,passed,adapters,fingerprint,...}`（原子写：.tmp → fsync → rename），`PROTOCOL_VERSION` 不匹配即拒收（fail closed）。父进程轮询 10ms，超时 kill；Windows 上把子进程绑进 `KillOnCloseJob`（Job Object `KILL_ON_JOB_CLOSE`），父进程崩溃时 OS 连带杀探针。

## 5. core 组（crates/core/）

### lumino-core：共享领域类型
- `ViewState`：`scroll_x/y`、`zoom_x/y`（Pixels per Tick / per Key）、`ppq`（默认 1920）、`total_ticks`、`key_count`，内嵌 `SmoothScrollAnimation`（指数插值，factor=0.35）。
- `types.rs`：`Tool`（12 种编辑工具枚举）、`NotePrecision`（全音符…128 分音符）、`AudioAction`。
- `error.rs`：`CoreError`（thiserror）+ `Result<T>` 别名。thiserror ≈ 用宏自动生成 exception 类的 `what()`。
- `storage/`：`Config`（主题/语言/合成器后端/音色库路径）、`UiState`（窗口几何，逻辑像素口径 v2）。
- `i18n`：gettext 风格，原文即 key；`CURRENT_LANGUAGE: AtomicU8` 无锁读；宏 `tr!("原文")` / `trf!("{name}", name=v)`。

### lumino-diagnostics：内存治理
- **memtrace**：`#[global_allocator] TaggedAlloc`（后端 mimalloc / macOS jemalloc）。每次 `alloc` 在用户数据前插 `Header{tag,user_size}`，`dealloc` 逆推归因。标签是线程局部的（`thread_local Cell<AllocTag>`），`with_tag(tag, closure)` 设置并自动恢复（可嵌套）。标签：`Midi/SoundFont/Audio/Gpu/Ui/Other`，各一个 `AtomicIsize` 计数器。显存不走 Rust 分配器，由 `gpu_resource_tracker` 单独上报。
- **memory_monitor 两级 OOM 防护**：主监控线程每 100ms 读 RSS，≥95% 总内存直接 `abort()`（留余量保证 abort 能执行）；看门狗线程**完全独立**（50ms 轮询、独立 RSS 读取、SIGKILL/TerminateProcess），仅在 MIDI 加载期间生效。大分配前还有同步 `MemoryMonitor::check()`。

### lumino-message：消息中枢
- `enum Message<W,S,Se,T>`：全应用唯一"事件总线类型"（巨型 tagged union），泛型由 ui 层实例化为各子系统事件。变体含 `Core(Event)`、`Window/Sidebar/Settings/Toolbar(...)`、`EditorAction`、`CloudAction` 等 20+ 领域 Action。有体积守卫测试（变体 ≤200 字节）。
- `events.rs`：全局事件缓冲 `EVENT_BUFFER: OnceLock<Mutex<VecDeque<Event>>>` + `EVENT_WAKER: OnceLock<Box<dyn Fn()>>`。任意线程 `emit(Event)` → 入队 → waker（main.rs 注册的 proxy 发 `UiWake`）→ winit 唤醒 → `take_events()` 排空转成 Message。这是"后台线程 → UI"的标准通道。

### lumino-project：.lmpj 工程格式
- `LuminoProject`：`tracks: Vec<TrackSlot>` + tempo/拍号/CC/歌词/标记等 `(tick, track_id, ...)` 元组数组 + `shapes: Vec<ShapeModel>`。
- `TrackSlot` 三态枚举：`Unloaded{path}`（懒加载）/ `Loaded(data)` / `Modified(data)`（脏标记决定写回）。
- 保存：临时目录完整写入成功后整体换入（防写坏原工程）；逐部件写 `tempo.lmtemp`、`shapes.lmshapes`、`data/tracks/{:03}.lmtrack` 等。单文件形态 = 魔数 `LMPJ` + zstd + CRC32 打包同一批部件。

### shape 数据模型（Spiderweb 移植）
- `ShapeModel`：`kind: ShapeKind`（Line/Polyline/Freehand/Curve/Arc/Custom/Funnel）+ `geometry` + `style` + `ops: Vec<ShapeOp>` + `k: f32`（beats-per-key，必须随形状存否则换缩放走形）。
- `ShapeOp`：有序修饰链，23 种算子（Glue/Chop/Strum/Mask/Tiles/VelocityEnv…），分两阶段：`OpStage::Geometry`（改路径，音符生成前）/ `OpStage::Notes`（生成后改音符）。
- 落盘纪律：bincode 枚举变体只许追加到末尾；字段定宽（u16/u64 不用 usize）保证跨平台字节一致。

## 6. audio 组（crates/audio/）

### lumino-midi-model：文档模型（数据权威）
- `NoteEvent`：`#[repr(C)]` 12 字节 POD——`start_tick/end_tick: u32` + `key/velocity/release_velocity/channel: u8`。音符即值本身，无全局 ID（落盘丢 ID、回读漂移是已知语义）。
- `MidiDocument`：`notes: Vec<ChunkedList<NoteEvent>>`（每轨一个，按 start_tick 升序）+ tempo/CC/歌词等 + 每轨 `max_end_tick` 惰性缓存。手写 `Clone`：音符块共享、缓存单元强制独立，防快照与实时文档串数据。
- `ChunkedList<T>`（核心数据结构）：
  ```rust
  chunks: Vec<Arc<Vec<T>>>,    // 每块 ≤50 万事件，Arc 共享（≈shared_ptr）
  chunk_first_ticks: Vec<u32>, // 块级二分索引
  chunk_offsets: Vec<usize>,   // 前缀和，定位全局索引
  ```
  为何分块：单一 `Vec` 有序插入是 O(N) memmove，1600 万音符会卡 UI；分块后插入只动块内。为何 `Arc`：clone 退化为 O(块数) 指针拷贝，修改经 `Arc::make_mut` 只复制目标块（写时复制）——撤销快照因此不拷整轨。
- `multi_port`：MIDI 端口 → 全局通道映射 `global = port*16 + channel`（16 端口 / 256 通道上限）。

### lumino-note-core：编辑器领域模型
- `History`：undo/redo 双栈。方案 = 快照 + 操作日志混合：`EditorSnapshot`（块级 COW 共享）用于结构性操作；`MoveOp`（`originals` + `moved` 两组 NoteEvent，按值定位不存索引）用于移动。合并策略：NoteCreate 300ms 合并窗口；拖动松手才提交；超 1000 条分组用 `parent_group_id` 串联成逻辑操作。
- `NoteSpatialIndex`：变体二叉树（数组存储非指针）。分割轴 = tick 二分，叶内按 key 排序（叶容量 128）；查询先 tick 区间裁剪再 key 二分。服务于命中检测与框选。

### lumino-midi-loader：文件加载
- 两条路径：**全量** `MidiDocument::from_notes_bytes`（midly 流式逐轨回调，每轨排序后直接 `ChunkedList::from_sorted_iter`，2.9 亿音符省 ~4.6GB 中间峰值）；**流式** `StreamingMidiPlayer`（mmap 零拷贝，每轨一个游标，最小堆归并交织，零事件常驻，供音频导出）。
- `archive.rs`：ZIP/RAR/7z/TAR/GZ/XZ/LZH/ISO 解压（unarc-rs）。
- `.lmpj` = bincode + zstd 自包含序列化。加载前 `MemoryMonitor::check()` 防 OOM。

### lumino-midi-io：I/O 抽象 + 播放管线
- trait 层级（≈ C++ 抽象基类）：`Api`（设备枚举/开关/音频流自愈）→ `SynthControl`（能力扩展）→ `OutputConnection`（note_on/control_change/pitch_bend…）→ `PlaybackOutput`（延音释放、统计）。四个后端实现：`api/xsynth.rs`（CPU SoundFont 合成）、`api/kdmapi.rs`（`libloading` 动态加载 OmniMIDI.dll，v1/v2 兼容）、`api/system.rs`（WinMM 系统 MIDI）、`api/lgs.rs`（包装 GPU 合成器）。
- 播放线程流程：UI 发 `Command`（Play/Seek/…）到专用播放线程（std mpsc）→ 每帧 `PlaybackEngine::update()`：读当前 tick（墙钟推算）→ 从文档流式游标读音符（NoteOn 直接发；NoteOff 按 end_tick 放最小堆）→ 迟到 >0.15s 丢弃（防补发洪水）→ 产出 `Vec<MidiMessage>` 给后端。
- UI 不抢播放锁：播放线程每帧经 crossbeam `bounded(8)` 环形通道 `try_send` `PlaybackFrame`（tick/BPM/16 通道电平表），满则丢旧，UI 只 `try_recv`。
- `soundfont_cache.rs`：全局 SF2 缓存，key=(路径,采样率)，校验 mtime。
- XSynth 线程模型：**每通道一个渲染线程**（16 通道 16 线程），刻意不用线程池（实测队头阻塞 + 丧失跨 voice 批渲染优化）。

### lumino-gpu-synth：GPU 计算着色器合成器
- 事件压缩：`TimedEvent` 8 字节（`sample: u32` + `packed: u32` = channel 8bit ‖ kind 4bit ‖ payload 20bit），sample 由 tempo 图换算成绝对采样位置。
- CPU 侧：事件按块调度、voice 分配/偷取（5ms 淡出）、SF2 解析、采样数据一次性上传（4 个 1GiB 存储 buffer 分块）、每块上传 `VoiceParams`（采样偏移/循环/amp/pan/biquad 系数）。
- GPU 侧两条 compute 管线：`render.wgsl` 一次 invocation 渲染一个 voice 整块（`sample × amp × pan × envelope → biquad 低通`，插值线性或 64 点 sinc）；`mix.wgsl` 每帧一个线程求和所有 voice，控制事件做 10ms 线性平滑，输出与块大小无关。
- 实时流水线：渲染线程按块墙钟预算（90%）渲染，块进有界采样队列（≤600ms）；OS 音频回调只从队列拷贝，欠载写静音，**零锁零分配**。TDR 走钩子 → 先建新引擎成功才热替换。

### lumino-midiplayer：贴图瀑布流
- 把全曲音符预渲染成贴图矩阵：时间维每 4 小节一张（1920px 宽），音轨维每 8 轨一组 + 整合组（后轨覆盖前轨）；RGBA8、1 key = 1px、Nearest 采样。可见范围 <40% 全曲时启用，磁盘缓存 `.lmocache`（zstd）。
- 背压设计：后台生成线程经 `sync_channel(1)` 发完成消息，容量 1 满则阻塞——防大 MIDI 内存峰值。编辑后发 `WaterfallDirtyRegion`（轨 + 受影响 time_group），只重生受影响贴图。

## 7. gfx（crates/editor/gfx）：wgpu 渲染层

### 关键文件
| 路径（src/ 下） | 职责 |
|---|---|
| `swappable_buffer.rs` | UI↔渲染线程三缓冲零拷贝交换 |
| `render_thread.rs` + `render_thread/` | 渲染线程：命令通道、`RenderCommand{Render{params,frame_id}, Control}` |
| `render_thread/params.rs` | `RenderParams`：一帧全部 CPU 侧输入（视口/颜色/实例 Vec/模式开关） |
| `note_renderer.rs` | 主音符 + 洋葱皮渲染器：GPU 常驻实例 + compute cull + draw_indirect |
| `gpu_note_buffer.rs` | `GpuNoteBuffer`：音符常驻 GPU 缓冲，CPU 无影子副本 |
| `pipeline.rs` / `shader.rs` / `shaders/` | 管线构建样板 / WGSL 入口 / 22 个着色器 |
| `cache.rs` | `RenderCache`（UI 侧）：视口哈希 + 深度纹理 + 走带实例缓存 |
| `grid_renderer.rs` / `ruler_renderer.rs` / `cc_bar_renderer.rs` | 无限网格（fragment shader O(1) 画线）/ 标尺 / CC 柱 |
| `arrangement_renderer.rs` / `miditrail_renderer.rs` / `waterfall_renderer.rs` | 走带 / Miditrail 3D / 瀑布流 |
| `context.rs` | `Surface` + 进程级 `SharedGpu` + 重连工作线程 |
| `device_status.rs` / `epoch.rs` / `frame_error.rs` | TDR 全局标志 / `GpuEpoch` 代次判废 / 帧错误分类 |

### 线程间数据交换：`SwappableBuffer<T>`
3 个槽（writer/ready/reading）+ `AtomicU32` 打包三槽状态 + CAS 整体置换。UI 线程 `write_buffer()+swap()`，渲染线程每帧一次 `acquire_read_buffer()`。三角色恒指不同物理槽，无锁读写隔离。版本号 `AtomicU64` 判"变没变"，没变不上传。

### 音符 GPU 实例格式（`NoteInstance`，16 字节，`repr(C)` + bytemuck）
```
start_length: [f32;2]  // [start_tick, length_tick]
key_color:    u32      // 低8位 = MIDI key，高24位 = RGB
border_width: u32      // 低16位 = 边框像素宽；高16位 = 轨道深度编码
```
`border_width==0xFFFFFFFF` 是预览哨兵。每实例 4 顶点 TriangleStrip。compute cull（`cull.wgsl`）输出可见实例的 u32 源索引 + `DrawIndirectArgs`（instance_count 由 GPU 写入），draw 时按索引从 storage buffer 取完整数据。

### 一帧流程（钢琴卷帘）
1. UI 线程算 `RenderParams`；音符走三缓冲 swap 或 `send_note_event()` 增量；经无界 mpsc 发 `Render{params, frame_id}`。
2. 渲染线程 drain 命令**只保留最后一条 Render**（积压合并）；处理 resize/视频导出/瀑布流/洋葱皮流式通道/note 增量事件。
3. 确保离屏 color+depth 纹理匹配视口（复用优先）；三缓冲 version 变化才上传实例。
4. `prepare_renderers` 写 uniform / 上传网格标尺 CC 实例。
5. 单 encoder 一个 render_pass 依序画：网格 → 洋葱皮 → 瀑布流 → 主音符 → 标尺 → CC 柱（各自 scissor）；多层靠 depth + 提交序。
6. submit；发布离屏纹理句柄；`frame_sync` Condvar 通知 `rendered_frame = frame_id`。
7. UI 线程 `wait_for_frame`（100ms 超时）→ 离屏纹理 copy/blit 到 Surface → present。交换链 `maximum_frame_latency=1` 做背压。

### device-lost 恢复（配合壳层 §4 的状态机）
device-lost 回调只做原子置位 + 无锁通知（回调线程禁止 wgpu 调用/锁）→ 渲染线程主循环首行检测到即有序退出 → UI 侧重建 device 并**递增 `GpuEpoch`** → 全部 `EpochTagged<T>` 跨帧资源代次不符即重建 → 重 spawn 渲染线程。

## 8. editor UI 组

### iced 模型（本项目的用法）
Elm 架构**保留模式**：widget 树每帧从状态重建；输入转成 `Message`（tagged union）→ `update()` 改状态 → `view()` 重绘。项目**不用** `iced::application` 托管循环，而是自持 winit 循环（`Runner`），`Host`（ui/src/host.rs）手动驱动 iced runtime。

### editor-state：纯状态层
- `EditorState`：`view: ViewState` + `canvas: CanvasState` + `interaction: InteractionState` + `tool: Tool`。
- `enum EditState`（交互状态机核心判别式）：`Selecting / DraggingSelection / DraggingSelectionCopy / 画刷/画笔/形状…`。
- `EditorData`：持有 `document`（唯一权威）+ `current_track` + `track_notes_gen`（写版本号，缓存失效键）。
- `trait EditorTransform`：`flip/transpose/apply_speed_change`，每个操作前 `push_history()`。

### ui-editor：钢琴卷帘主体
- `struct Editor`：`editor_state` + `velocity_panel` + 三种 `canvas::Cache`（网格/键盘/标尺位图缓存，`CacheInvalidation` 位标志控制）+ `spatial: SpatialIndexState`（惰性 R-tree，以 `(track_notes_gen, current_track)` 为失效键）+ `pending_drag_state`（ghost 延迟提交：松手不立即写 document，避免连续拖动重建索引）+ `remote_cursors`（协作远端光标）。
- 关键子模块：`grid/program.rs`（iced canvas `Program` trait 实现，收鼠标事件）、`interaction/`（pressed/moved/released 分发）、`drag/`（吸附与 delta 计算）、`note_ops/`、`clipboard/`（arboard 私有 JSON，含 Domino 格式互通）、`recording.rs`、`velocity/`（力度面板 X 轴与卷帘联动）。

**一次拖动音符的完整链路**：
1. winit 鼠标按下 → iced canvas `Program::update`（`grid/program_impl.rs`）命中分类。
2. canvas 不改数据，只发 `Message::EditorAction(EditorAction::Pressed{pos})`。
3. 消息上浮 `Host::route_message` → `Root::update`（handlers.rs）→ `Editor` 的 match（`interaction.rs`）。
4. `handle_pressed` 经空间索引命中，`edit_state` 切到 `DraggingSelection`。
5. 移动 → 同链路 `Moved` → `drag::compute_state_changes`（吸附 snap_precision），期间只画 ghost 不写 document。
6. 松手 `Released` → delta 存 `pending_drag_state` 或直接写 document + `push_history()`；`track_notes_gen` bump。
7. 重绘：`view()` 重建 widget 树 + canvas Cache 增量；gfx 渲染线程按增量事件上 GPU。

### ui：应用外壳
- `struct Host`（host.rs）：渲染上下文 + 窗口上下文 + `root: Root`，手动驱动 iced。
- `struct Root`（root.rs + `root/handlers.rs`）：聚合全部子组件（titlebar/toolbar/sidebar/right_sidebar/editor/settings/playback/toast/mixer_panel/cloud…）。`update(message)` 总分发：直接处理 + `MessageRouter` 路由到专用 handler。
- `wgpu_render_thread.rs`：渲染线程的 UI 侧句柄（命令通道、RenderParams/RenderStats）。
- waker 机制见 §5 lumino-message。

### dialog：多窗口
`DialogManager`：`HashMap<WindowId, DialogWindow>`，`claimed_windows` 按窗口归属分派 winit 事件防串台。每个 `DialogWindow` = 独立 winit 窗口 + **独立 gfx Context + 独立 Host**（完整迷你 UI 循环）。隐藏窗口先测量内容再定尺寸显示。

### ui-settings
状态集中在 `SettingsPanel`（含 `SettingsEvent` 枚举），`pages/*.rs` 每页一个无状态 `view()` 函数。

## 9. network 组

### lumino-collaboration：协作客户端
- `CollaborationClient`（拆 8 个子模块：connection/message/handlers/state…）。
- 流程：HTTP（reqwest）建房/查房 → WebSocket（`tokio-tungstenite`）连 Cloudflare Durable Object → 首帧 JSON `Auth{username,password}` → 服务器广播他人操作（`MouseMove`、`NoteBatch`）→ `handle_server_message` 反序列化分发 → `CollaborationEvent` 回调 UI。帧格式 = UTF-8 JSON 文本（serde camelCase）。上传走分块流（有界 mpsc，避免整份复制）。

### lumino-cloud：三协议云存储
统一抽象 = `trait CloudClient: Send`（对象安全，`Box<dyn CloudClient>`）：`connect / list_dir / upload_file / …`；实现：`ftp.rs`（suppaftp）、`sftp.rs`（russh + russh-sftp）、`webdav.rs`（reqwest_dav + 手写 XML 目录解析）。
`CloudManager` 对外**同步阻塞**接口，内部 `block_on` 全局 tokio Runtime。凭据加密 AES-256-GCM（密钥为编译期内置常量——防"打开 cloud.json 看到明文"，非真安全，逆向可得）。

## 10. tools 组

### lumino-export：导出
- **视频导出（离线渲染，非录屏）**：`FfmpegEncoder` 启动 ffmpeg 子进程（stdin pipe，rawvideo 输入，`Command::new("ffmpeg")` 从程序目录+PATH 发现）；渲染器（gfx 侧，用 `waterfall.wgsl`/`miditrail_*.wgsl` compute 写离屏纹理）逐帧产出 BGRA 像素 → `write_frame(Vec<u8>)` 写 1MB BufWriter → mp4。Drop 自动 kill 子进程（取消场景）。字幕/计数器用 `ab_glyph` CPU 光栅化（中文支持）。
- **音频导出**：双模式（流式 / 内存），事件流 → `AudioEngine → EventProcessor → SampleSink`（参考 OmniConverter）；GPU 路径统一走 `GpuSynth::render_midi_bytes`。
- **MIDI 导出**：`midly` 构建 SMF。
- `.lmpj` 容器：`encode_lmpj` = bincode + zstd(level 3)。

### lumino-extras：杂项
- i18n 双层：编译期强类型翻译表（主界面骨架）+ 运行时原文即 key 目录（长尾），宏在 lumino-core。
- 调色板：`resources/palettes/` PNG（每像素一色，参考 Zenith-MIDI），`PALETTE_MANAGER` 全局单例。素材 `.lmmaterial` 编译时嵌入（include_bytes 模式）。

## 11. Rust → C++ 速查表

| Rust 写法 | C++ 等价理解 |
|---|---|
| `struct Foo` + 另写 `impl Foo { fn bar(&self) }` | 类定义与成员函数分离；`&self` = const this，`&mut self` = this |
| `trait` | 纯虚接口类（可带默认实现） |
| `Box<T>` / `Arc<T>` | `unique_ptr` / `shared_ptr` |
| `Arc::make_mut` | 写时复制：共享则先克隆 |
| `Option<T>` / `Result<T,E>` + `?` | `optional` / `expected`；`?` = 出错即 return |
| `enum E{A(i32),B}` + `match` | tagged union + `std::visit`，编译器强制穷举 |
| `Vec<T>` / `HashMap` | `vector` / `unordered_map` |
| `Mutex<T>` | 互斥量**包着数据**，访问必须先锁 |
| channel（mpsc/crossbeam） | 线程安全消息队列（生产者-消费者） |
| `#[derive(Debug, Clone)]` | 自动生成 to-string / 拷贝构造 |
| `#[repr(C)]` + bytemuck | 内存布局固定的 POD，可 reinterpret 直传 GPU |
| `#[cfg(target_os="windows")]` | `#ifdef _WIN32` |
| `#[global_allocator]` | 替换全局 operator new |
| `thread_local!` / `OnceLock` | thread_local / call_once + static |
| `#[test]` + `#[cfg(test)] mod tests` | 内联单元测试；`tests/` 目录 = 独立测试目标 |
| `pub` / `pub(crate)` / 无修饰 | public / internal / 文件内可见（≈匿名命名空间） |
| crate 名（`lumino_gfx::cache`） | namespace + 链接的库 |
| `tokio` async / `.await` | 协程运行时 / co_await（仅网络层用） |
