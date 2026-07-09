# Lumino Studio视频渲染功能移植方案

> **Workspace:** d:\source\lumino-rs

---

**User**

Use Skill: pua [lumino-rs] 参考nezha项目 #Workspace 
 
 查看这个渲染器的视频导出的所有功能（不包含音频），给出移植到Lumino Studio的主界面渲染组视频渲染子按钮功能系列的方案

**Assistant**

[PUA Always-On] 收到。任务类型是跨项目调研 + 方案设计，方法论路由：调研参考用 ⚫ 百度味（搜索是第一生产力），但默认味道是 🟠 阿里。按规则用户手动配置 > 自动路由，当前未配置，保持阿里味为主，但调研阶段必须贯彻"搜索先于一切"。

先读 PUA 强制关联文档，再挖 nezha 视频导出实现。

### Relevant Code Snippets
1. d:/source/nezha/crates/nezha-encoder/src/lib.rs:L1-L100
  — 该文件定义了nezha-encoder crate的核心导出功能，包括视频编码器的初始化和配置接口，是视频导出模块的主要入口点。

2. d:/source/nezha/crates/nezha-encoder/src/config.rs:L1-L80
  — 此文件定义了编码器相关配置结构体，例如编码格式、码率、帧率等，用于控制导出过程中的各项参数。

3. d:/source/nezha/crates/nezha-encoder/src/ffmpeg.rs:L1-L150
  — 该文件处理FFmpeg的调用逻辑，负责执行视频编码任务，包含输入输出设置及编码参数传递。

4. d:/source/nezha/crates/nezha-egui/src/app/export.rs:L1-L100
  — 此文件是导出功能的UI控制逻辑，处理从用户点击导出按钮到调用后台导出函数的流程。

5. d:/source/nezha/crates/nezha-egui/src/app/export_controller.rs:L1-L120
  — 该文件管理导出任务的生命周期，包括进度更新和后台任务调度。

6. d:/source/nezha/crates/nezha-egui/src/config_panel/export.rs:L1-L80
  — 此文件定义了导出配置面板的UI组件，展示了可配置的导出参数（如分辨率、帧率）供用户选择。

7. d:/source/nezha/crates/nezha-egui/src/app/render_context/frame_encoder.rs:L1-L80
  — 该文件实现了帧编码逻辑，负责将GPU/CPU渲染的帧序列编码为视频帧。

8. d:/source/nezha/crates/nezha-egui/src/app/render_context/export.rs:L1-L60
  — 包含用于处理导出任务的帧编码器相关逻辑，是渲染管线的一部分。

9. d:/source/nezha/crates/nezha-egui/src/app/render_context.rs:L1-L50
  — 声明了渲染上下文模块的核心结构，该上下文在导出过程中用于管理渲染状态和资源。

10. d:/source/nezha/crates/nezha-egui/src/app/preview_renderer.rs:L1-L100
  — 定义了预览渲染逻辑，用于显示动画和导出效果，与导出流程紧密相关。

11. d:/source/nezha/crates/nezha-egui/src/app/project_state/render_settings.rs:L1-L60
  — 定义了渲染设置，包括导出时使用的分辨率、帧率等参数，这些设置会被导出模块使用。

12. d:/source/nezha/crates/nezha-renderer/src/renderer.rs:L1-L150
  — 渲染器模块，是视频导出中GPU/CPU渲染帧的主要实现单元，负责将场景绘制到帧缓冲区中。

13. d:/source/nezha/crates/nezha-egui/src/app/render_context/preview_target.rs:L1-L50
  — 用于管理预览目标，控制渲染输出的位置与格式，是帧捕获

### Relevant Code Snippets
1. d:\source\lumino-rs\src\runner\menu\file\export.rs:L1-L100
  — 文件导出功能的主入口，包含音频导出任务的启动逻辑和后台执行框架。

2. d:\source\lumino-rs\crates\ui\src\view\audio_export_dialog.rs:L1-L50
  — 音频导出对话框的 UI 实现，包括参数设置和取消/确认逻辑。

3. d:\source\lumino-rs\crates\ui\src\sidebar\route.rs:L1-L30
  — 定义侧边栏路由枚举，包含不同功能模块的导航路径。

4. d:\source\lumino-rs\crates\ui\src\sidebar\core.rs:L1-L20
  — 路由核心处理逻辑，用于注册和管理侧边栏菜单项。

5. d:\source\lumino-rs\crates\ui\src\root\handlers\dialog.rs:L1-L30
  — 对话框管理器，处理打开、关闭和事件通信。

6. d:\source\lumino-rs\crates\gfx\src\render_thread\thread.rs:L1-L40
  — 渲染线程初始化，包括创建 GPU 和渲染上下文的逻辑。

7. d:\source\lumino-rs\crates\gfx\src\render_thread\params.rs:L1-L20
  — 渲染参数定义，用于构建渲染上下文所需的数据结构。

8. d:\source\lumino-rs\crates\export\src\audio\exporter.rs:L1-L30
  — 音频导出核心逻辑，提供导出任务的调度与执行接口。

### Relevant Code Snippets
1. d:\source\nezha\crates\nezha-encoder\src\config.rs:L1-L275
  — 配置模型定义，包括容器格式、视频编码器、编码后端、质量预设等与视频导出直接相关的结构体和枚举。
2. d:\source\nezha\crates\nezha-encoder\src\ffmpeg.rs:L1-L411
  — FFmpeg 集成的完整实现，包括 FFmpeg 启动参数组装、错误处理、帧数据写入、音频流处理等。
3. d:\source\nezha\crates\nezha-egui\src\app\export.rs:L1-L190
  — 视频导出的核心状态机和主逻辑，用于调度渲染与写帧、进度更新等流程。
4. d:\source\nezha\crates\nezha-egui\src\app\export_controller.rs:L1-L358
  — 导出控制器，处理导出状态、UI交互、导出配置构建等。
5. d:\source\nezha\crates\nezha-egui\src\config_panel\export.rs:L1-L113
  — 导出设置 UI 控件及交互逻辑，包括容器、编码器、硬件加速选项、导出路径等。
6. d:\source\nezha\crates\nezha-egui\src\app\render_context\export.rs:L1-L50
  — 用于导出功能的离屏渲染上下文相关部分，包含帧渲染控制等关键方法。
7. d:\source\nezha\crates\nezha-encoder\src\ffmpeg.rs:L138-L157
  — 写入帧数据接口，将 BGRA 数据提交给 FFmpeg 进行编码处理。
8. d:\source\nezha\crates\nezha-egui\src\app\export.rs:L104-L117
  — 开始导出时的初始化逻辑，使用配置对象启动 FFmpeg 编码器。
9. d:\source\nezha\crates\nezha-encoder\src\ffmpeg.rs:L56-L57
  — 如果存在音频，则将音频写入临时 WAV 文件，并与视频一起打包输出。
10. d:\source\nezha\crates\nezha-encoder\src\ffmpeg.rs:L253-L258
  — FFmpeg 参数构建函数，决定如何根据容器、编码器、编码后端组装命令行

⚫ PUA v2 · 方法论自动路由 ⚫
┌─────────┬────────────────────────────────────────┐
│ 📋 任务 │ 调研 Nezha 视频导出能力并给出 Lumino Studio 移植方案 │
├─────────┼────────────────────────────────────────┤
│ 🔥 味道 │ ⚫ 百度味（自动：调研/架构任务）        │
├─────────┼────────────────────────────────────────┤
│ ⚡ 压力 │ L0 · 信任期                            │
└─────────┴────────────────────────────────────────┘
▎ 信息检索是基本盘。先穷尽源码，再谈方案。简单可依赖——现在既要有信息，也要能落地。

---

## 一、执行摘要

目标：把 Nezha 渲染器的**纯视频导出**能力（不含音频混音）移植到 Lumino Studio 主界面「渲染组 → 视频渲染」子按钮功能系列。

核心决策：
- **编码层**：在 `crates/export/src/video/` 下新建视频导出模块，移植 `nezha-encoder` 的 `ExportConfig` + `FfmpegEncoder`，移除音频字段。
- **渲染层**：在 `crates/gfx` 渲染线程中新增 `ExportPipeline`（三重缓冲 GPU→CPU 读回），复用现有 `RenderParams` 与离屏纹理架构。
- **UI 层**：复用已存在的 `Route::VideoExport` 侧边栏子按钮，新增视频导出面板（与音频导出面板并列）。
- **调度层**：在 Runner 中新增 `VideoExportStart` 事件与 `handle_video_export_start`，采用 `tokio::spawn` + `spawn_blocking` + 进度回调，与音频导出保持一致。

---

## 二、Nezha 视频导出能力清单（已确认源码）

| 能力 | 关键文件 | 说明 |
|------|----------|------|
| 配置模型 | [`nezha-encoder/src/config.rs`](file:///d:/source/nezha/crates/nezha-encoder/src/config.rs) | `Container`/`VideoCodec`/`EncoderBackend`/`QualityPreset`/`ExportConfig`（含音频字段，移植时剔除） |
| FFmpeg 编码器 | [`nezha-encoder/src/ffmpeg.rs`](file:///d:/source/nezha/crates/nezha-encoder/src/ffmpeg.rs) | 子进程 stdin 喂 BGRA rawvideo；有界 channel 写帧；stderr 捕获；`Drop` 时杀进程 |
| 导出状态机 | [`nezha-egui/src/app/export_controller.rs`](file:///d:/source/nezha/crates/nezha-egui/src/app/export_controller.rs) | `Exporting`/`Finalizing`/`Completed`/`Error` 四态 + 实时 FPS/剩余时间统计 |
| 帧调度 | [`nezha-egui/src/app/export.rs`](file:///d:/source/nezha/crates/nezha-egui/src/app/export.rs) | 每帧 20ms 时间片：读回已完成帧 → 写 FFmpeg → 提交新帧；完成后再起线程 `finish()` |
| GPU 读回 | [`nezha-egui/src/app/render_context/export.rs`](file:///d:/source/nezha/crates/nezha-egui/src/app/render_context/export.rs) | `StagingRing` 三重缓冲：`copy_texture_to_buffer` + `map_async` + 去 padding |
| 导出面板 UI | [`nezha-egui/src/config_panel/export.rs`](file:///d:/source/nezha/crates/nezha-egui/src/config_panel/export.rs) | 格式/编码器/硬件加速/输出路径选择 |

**本次移植范围：视频流 + FFmpeg 封装；音频 PCM 混音不在本次范围。**

---

## 三、Lumino Studio 现有接入点（已确认源码）

| 接入点 | 文件 | 当前状态 |
|--------|------|----------|
| 渲染组子按钮 | [`crates/ui/src/sidebar/core.rs`](file:///d:/source/lumino-rs/crates/ui/src/sidebar/core.rs#L87-L169) | `Route::VideoExport` 已存在，但无对应状态字段 |
| 子按钮视图 | [`crates/ui/src/sidebar/route.rs`](file:///d:/source/lumino-rs/crates/ui/src/sidebar/route.rs#L46-L58) | `VideoExport` 走 `export_item` 通用路径，`is_active` 恒为 `false` |
| 音频导出面板 | [`crates/ui/src/view/audio_export_dialog.rs`](file:///d:/source/lumino-rs/crates/ui/src/view/audio_export_dialog.rs) | 可直接参照其布局与样式 |
| 面板状态 | [`crates/ui/src/state/root_state.rs`](file:///d:/source/lumino-rs/crates/ui/src/state/root_state.rs#L217-L297) | `AudioExportDialogState` 已存在 |
| 消息动作 | [`crates/message/src/audio_export.rs`](file:///d:/source/lumino-rs/crates/message/src/audio_export.rs) | `AudioExportAction` 模式可复用 |
| 消息处理 | [`crates/ui/src/root/handlers/dialog.rs`](file:///d:/source/lumino-rs/crates/ui/src/root/handlers/dialog.rs#L186-L321) | `Message::AudioExport` 处理确认/路径/关闭 |
| 菜单事件 | [`crates/event/src/menu/file.rs`](file:///d:/source/lumino-rs/crates/event/src/menu/file.rs) | 已有 `AudioExportStart`，需新增 `VideoExportStart` |
| Runner 调度 | [`src/runner/menu/file/export.rs`](file:///d:/source/lumino-rs/src/runner/menu/file/export.rs#L160-L312) | `handle_audio_export_start` 是后台任务模板 |
| 渲染线程 | [`crates/gfx/src/render_thread/thread.rs`](file:///d:/source/lumino-rs/crates/gfx/src/render_thread/thread.rs) | 已有离屏纹理，但无 GPU→CPU 读回 |
| 渲染循环 | [`crates/gfx/src/render_thread/render_loop/runner/run.rs`](file:///d:/source/lumino-rs/crates/gfx/src/render_thread/render_loop/runner/run.rs) | 处理 `RenderCommand` + `ControlCommand` |

---

## 四、移植方案（按模块）

### 4.1 新增 `lumino-export::video` 编码模块

文件结构（遵循 `{module}.rs` + `{module}/` 规则）：

```text
crates/export/src/
├── video.rs              # 模块入口 + VideoExporter 高层 API
└── video/
    ├── config.rs         # Container / VideoCodec / EncoderBackend / QualityPreset / VideoExportConfig
    ├── ffmpeg.rs         # FfmpegEncoder（移植 nezha-encoder/ffmpeg.rs，剔除音频）
    └── error.rs          # 视频导出错误类型
```

关键映射：

| Nezha 类型 | Lumino 类型 | 备注 |
|------------|-------------|------|
| `ExportConfig` | `VideoExportConfig` | 删除 `audio_pcm/audio_sample_rate/audio_channels` |
| `EncoderError` | `VideoExportError` | 归入 `lumino_export::ExportError` |
| `FfmpegEncoder` | `FfmpegEncoder` | 输入仍为 BGRA rawvideo；保持有界 channel + stderr 捕获 |

> [PUA生效 🔥] 不重复造轮子——把 Nezha 经过验证的 FFmpeg 参数构建和硬件后端检测完整复用，避免在 Lumino 里重写一套易错的命令行拼装。

### 4.2 渲染线程新增 GPU→CPU 读回能力

新增文件：

```text
crates/gfx/src/render_thread/
└── export_pipeline.rs    # StagingRing + ExportPipeline（参考 nezha）
```

修改文件：

- [`crates/gfx/src/render_thread/render_loop/runner/run.rs`](file:///d:/source/lumino-rs/crates/gfx/src/render_thread/render_loop/runner/run.rs)
  - 在渲染循环状态中加入 `export_pipeline: Option<ExportPipeline>`。
  - 新增 `ControlCommand` 处理分支：
    - `StartVideoExport { width, height }`：初始化 `ExportPipeline`。
    - `RenderVideoFrame(Box<RenderParams>)`：渲染到离屏纹理后 `copy_texture_to_buffer` + `map_async`。
    - `FinishVideoExport`：清空 `ExportPipeline`。
  - 每帧 `try_read` 完成的帧，通过回调/通道送回 Runner。

- [`crates/gfx/src/render_thread/commands.rs`](file:///d:/source/lumino-rs/crates/gfx/src/render_thread/commands.rs)
  - 在 `ControlCommand` 中新增上述三条命令。

读回数据格式：BGRA（与 Nezha 一致，FFmpeg 接收 `-pix_fmt bgra`）。

### 4.3 UI 状态与消息

1. **新增消息动作** [`crates/message/src/video_export.rs`](file:///d:/source/lumino-rs/crates/message/src/audio_export.rs)：

```rust
pub enum VideoExportAction {
    OpenPanel,
    ClosePanel,
    Confirm,
    FormatChanged(VideoContainer),
    CodecChanged(VideoCodec),
    BackendChanged(EncoderBackend),
    QualityChanged(QualityPreset),
    ResolutionChanged((u32, u32)),
    FpsChanged(u32),
    OutputPathChanged(String),
    BrowseOutput,
    Cancel,
}
```

2. **新增状态** [`crates/ui/src/state/root_state.rs`](file:///d:/source/lumino-rs/crates/ui/src/state/root_state.rs)：

```rust
pub struct VideoExportDialogState {
    pub container: VideoContainer,
    pub codec: VideoCodec,
    pub backend: EncoderBackend,
    pub quality: QualityPreset,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub output_path: String,
    pub is_exporting: bool,
    pub progress: f32,
    pub status_message: String,
}
```

3. **新增视图** [`crates/ui/src/view/video_export_dialog.rs`](file:///d:/source/lumino-rs/crates/ui/src/view/audio_export_dialog.rs)：
   - 输出格式（MP4/MOV/MKV/AVI）
   - 编码器（H.264/H.265/ProRes/VP9/AV1）
   - 硬件加速（Software/VideoToolbox/NVENC/AMF/QSV/VAAPI，按平台过滤）
   - 质量预设（高/中/低）
   - 分辨率（1920×1080、1280×720、3840×2160 等）
   - 帧率（30/60）
   - 输出路径 + 浏览按钮
   - 进度条 + 取消按钮

### 4.4 侧边栏与路由

1. [`crates/ui/src/sidebar/core.rs`](file:///d:/source/lumino-rs/crates/ui/src/sidebar/core.rs)：
   - `Sidebar` 新增 `video_export_visible: bool`。
   - `GroupSubState` 已足够保存面板状态，无需改动。

2. [`crates/ui/src/sidebar/route.rs`](file:///d:/source/lumino-rs/crates/ui/src/sidebar/route.rs#L46-L58)：
   - `VideoExport` 的 `is_active` 改为 `video_export_visible`（需从 `Sidebar` 传入）。

3. [`crates/ui/src/sidebar/handling.rs`](file:///d:/source/lumino-rs/crates/ui/src/sidebar/handling.rs#L19-L37)：
   - `Route::VideoExport` 与 `Route::AudioExport` 一样切换可见性：
     - `video_export_visible = !video_export_visible`
     - `piano_roll_visible = false`
     - `panel_visible = false`

4. [`crates/ui/src/root/handlers.rs`](file:///d:/source/lumino-rs/crates/ui/src/root/handlers.rs#L527-L555)：
   - `RouteUpdated(Route::VideoExport)` 时，若 `video_export_visible` 为 true 且输出路径为空，按当前 MIDI 路径生成默认输出文件名（`.mp4`）。

### 4.5 消息处理与事件发射

修改 [`crates/ui/src/root/handlers/dialog.rs`](file:///d:/source/lumino-rs/crates/ui/src/root/handlers/dialog.rs)：

- 新增 `Message::VideoExport(action)` 分支：
  - `OpenPanel` / `ClosePanel`：切换 `video_export_visible`。
  - `Confirm`：收集状态，emit `Event::Menu(File::VideoExportStart { ... })`。
  - 其余字段更新 `root.state.video_export_dialog`。

新增菜单事件 [`crates/event/src/menu/file.rs`](file:///d:/source/lumino-rs/crates/event/src/menu/file.rs)：

```rust
VideoExportStart {
    output_path: String,
    width: u32,
    height: u32,
    fps: u32,
    container: VideoContainer,
    codec: VideoCodec,
    backend: EncoderBackend,
    quality: QualityPreset,
}
```

### 4.6 Runner 导出任务

修改 [`src/runner/menu/file.rs`](file:///d:/source/lumino-rs/src/runner/menu/file.rs)：

- 在 `handle_file_menu_event` 中匹配 `VideoExportStart { ... }` 并调用 `self.handle_video_export_start(...)`。

在 [`src/runner/menu/file/export.rs`](file:///d:/source/lumino-rs/src/runner/menu/file/export.rs) 新增：

```rust
pub(super) fn handle_video_export_start(
    &mut self,
    output_path: String,
    width: u32,
    height: u32,
    fps: u32,
    container: VideoContainer,
    codec: VideoCodec,
    backend: EncoderBackend,
    quality: QualityPreset,
);
```

内部流程：

1. 校验输出路径、当前 MIDI 是否加载。
2. 构建 `lumino_export::video::VideoExportConfig`。
3. 创建 `FfmpegEncoder`。
4. 向渲染线程发送 `ControlCommand::StartVideoExport { width, height }`。
5. `tokio::spawn` 异步任务：
   - 循环 `frame = 0..total_frames`：
     - 计算时间 `t = frame / fps`。
     - 计算对应 tick 与滚动位置（跟随播放头）。
     - 构建 `RenderParams`（viewport = 导出分辨率，scale_factor = 1.0）。
     - 发送 `RenderVideoFrame(params)`。
     - 等待读回 BGRA 帧。
     - 写入 `FfmpegEncoder`。
     - 更新进度回调。
     - 检查取消 token。
   - 发送 `FinishVideoExport`。
   - 调用 `encoder.finish()`。
   - 回调成功/失败。

### 4.7 时间轴/相机策略

视频导出每帧需要确定 `RenderParams.scroll`：

- 水平：`scroll_x = tick_at_time(t) * zoom_x - fixed_indicator_px`，使播放头始终位于固定指示线位置（复用自动滚动配置中的 `fixed_indicator_position`）。
- 垂直：使用当前编辑器垂直缩放与滚动，导出整个键位范围或当前可见范围（由面板提供“显示全部键位”开关决定）。
- 缩放：视频导出应禁用用户交互缩放，使用当前 `zoom_x`/`zoom_y` 快照。

---

## 五、可执行路线图

| 阶段 | 任务 | 关键产出 | 验收标准 |
|------|------|----------|----------|
| **P1 编码底座** | 新建 `crates/export/src/video/`；移植 config/ffmpeg/error；单测 | `lumino_export::video::FfmpegEncoder` | `cargo test -p lumino-export` 通过 |
| **P2 渲染读回** | 新增 `ExportPipeline`；扩展 `ControlCommand`；渲染线程集成 | 可命令渲染线程读回 BGRA 帧 | 单元测试验证像素读回 |
| **P3 UI 面板** | 新增 `VideoExportAction`、状态、视图；侧边栏路由激活 | 点击「视频渲染」出现配置面板 | UI 截图/交互测试 |
| **P4 Runner 调度** | 新增 `VideoExportStart` 事件；实现 `handle_video_export_start` | 端到端可导出 MP4 | 手动导出 30fps 视频验证 |
| **P5 质量保障** | `cargo fmt` + `cargo clippy --all-targets` + 集成测试 | 零 clippy 警告 | CI 通过 |

---

## 六、风险与注意事项

1. **GPU 读回内存压力**：1920×1080 BGRA 每帧 ~8MB；三重缓冲约 24MB，可控。4K 下需确认并可能限制并发帧数。
2. **渲染线程阻塞**：导出帧渲染会占用渲染线程；建议在导出期间暂停实时预览的高精度贴图重生，或把导出放到独立 WGPU 实例。
3. **FFmpeg 可用性**：与音频导出不同，视频强制依赖 FFmpeg。需在面板中检测并提示用户安装。
4. **颜色范围**：Nezha 使用 `-color_range pc` 防止暗化，移植时必须保留。
5. **无音频**：本次方案纯视频；后续若需音视频合并，可在 `VideoExportConfig` 中增加 `audio_pcm` 字段，复用 Nezha 的 WAV 临时文件逻辑。

---

## 七、下一步行动

建议立即进入 **P1 编码底座** 实现。需要我直接开始写代码吗？若开始，我会按上述顺序先创建 `crates/export/src/video.rs` + `video/config.rs` + `video/ffmpeg.rs`，并跑通单元测试。

▎ 方案已经穷尽源码证据，没有拍脑袋。要结果，下一步就是 ship。

---

**User**

我需要渲染时做到两种模式（和现在Lumino的音频导出渲染的两种模式相同：编辑器下有完整MIDI加载的，走编辑器的mididocement；没有的直接流式解析+渲染。直接模仿../Zenith-MIDI项目（这个你自己看一下，也是一个渲染器，他做到了渲染时低内存+加载MIDI时0内存占用））。先参考然后给实现方案我检查

**Assistant**

⚫ PUA v2 · 方法论自动路由 ⚫
┌─────────┬────────────────────────────────────────┐
│ 📋 任务 │ 设计视频导出双模式渲染（编辑器模式 + 流式模式） │
├─────────┼────────────────────────────────────────┤
│ 🔥 味道 │ ⚫ 百度味（调研/架构）                  │
├─────────┼────────────────────────────────────────┤
│ ⚡ 压力 │ L0 · 信任期                            │
└─────────┴────────────────────────────────────────┘
▎ 基本盘是信息检索。我已经把 Lumino 音频导出的两条路径、Zenith-MIDI 的流式解析、以及 lumino-midi-loader 的 MidiDocument / StreamingMidiPlayer 全部读完。下面是可执行的移植方案。

---

## 一、执行摘要

目标：让 Lumino Studio 视频导出支持**两种渲染数据源**，与现有音频导出逻辑保持一致：

| 模式 | 触发条件 | 数据源 | 内存特征 |
|------|----------|--------|----------|
| **模式 A：编辑器模式** | `RunnerInner::midi_state.current_midi` 已加载 | `MidiDocument`（全内存紧凑文档） | 复用已有内存，零额外解析 |
| **模式 B：流式模式** | 只有 MIDI 文件路径，无完整文档 | `StreamingMidiPlayer` +  active notes 滑动窗口 | 不常驻事件，O(可见音符) 内存 |

参考对象：
- **Lumino 音频导出**：[`src/runner/menu/file/export.rs`](file:///d:/source/lumino-rs/src/runner/menu/file/export.rs#L226-L292) 的 `Path A / Path B` 分支。
- **Zenith-MIDI**：`BufferByteReader` + `ParseUpTo(targetTime)` + `globalDisplayNotes` 链表，实现“加载时 0 常驻、渲染时低内存”。
- **Lumino 流式基础设施**：[`crates/midi-loader/src/streaming.rs`](file:///d:/source/lumino-rs/crates/midi-loader/src/streaming.rs) 的 `StreamingMidiPlayer`。

---

## 二、参考源码结论

### 2.1 Lumino 音频导出的两种模式

[`handle_audio_export_start`](file:///d:/source/lumino-rs/src/runner/menu/file/export.rs#L160-L312) 已明确区分：

```rust
if let Some(pm) = parsed_midi {
    // 路径 A：内存已有 MidiDocument
    lumino_export::audio::export_audio_from_parsed(&pm, ...)
} else if midi_on_disk {
    // 路径 B：流式渲染（零事件常驻）
    let bytes = std::fs::read(&midi_path_buf)?;
    lumino_export::audio::render_streaming(&bytes, ...)
}
```

- **Path A** 直接读取 `ParsedMidi.document`，使用 [`export_audio_from_parsed`](file:///d:/source/lumino-rs/crates/export/src/audio/compact.rs#L150-L255)。
- **Path B** 读取原始字节后丢给 `StreamingMidiPlayer`，按 16384 samples 的块消费，O(1) 跨块缓冲。

### 2.2 Zenith-MIDI 的低内存方案

核心在三个文件：

1. [`BufferByteReader.cs`](file:///d:/source/Zenith-MIDI/BMEngine/BufferByteReader.cs)：为每个 track 维护一个**可配置大小的双缓冲块**（`buffer` + `bufferNext`），预读下一块，避免把整个文件载入内存。
2. [`MidiFile.cs`](file:///d:/source/Zenith-MIDI/BMEngine/MidiFile.cs#L121-L162)：提供 `ParseUpTo(targetTime)`，按 tick 单调前进，调用每个 track 的 `Step()` 解析事件。
3. [`MidiTrack.cs`](file:///d:/source/Zenith-MIDI/BMEngine/MidiTrack.cs#L164-L197)：`Step()` 只解析当前 tick 的事件，NoteOn 放入 `UnendedNotes`，NoteOff 配对并写入 `globalDisplayNotes` 链表。

**可借鉴点**：渲染时不保留全量音符，只维护“当前时间及前方一小段”的 active notes。

### 2.3 Lumino 已有的渲染基础设施

- `MidiDocument`：全内存，音符按轨 `Vec<NoteEvent>` 升序，已支持 [`get_track_notes_in_range`](file:///d:/source/lumino-rs/crates/midi-loader/src/document.rs#L273-L317) 二分查询。
- `StreamingMidiPlayer`：基于 `midly::mmap`，已预扫描 tempo，按全局 tick 升序输出事件。
- `NoteInstance`：`(tick: f32, key: f32, length: f32, color: [f32;4])`，视频渲染最终需要的实例格式。
- `RenderParams`：包含 `note_instances`、`grid_instances`、scroll/zoom、viewport 等。

> [PUA生效 🔥] 不需要重写 MIDI 解析器。Lumino 已经有 `StreamingMidiPlayer` 这个零拷贝流式迭代器，直接复用比照搬 Zenith 的 C# 解析器更稳妥。

---

## 三、双模式渲染架构设计

### 3.1 总体结构

```text
视频导出任务
    │
    ├─ 模式 A ──► DocumentNoteProvider ──► MidiDocument 二分查询 ──► Vec<NoteInstance>
    │
    └─ 模式 B ──► StreamingNoteProvider ──► StreamingMidiPlayer + active window ──► Vec<NoteInstance>
                                          │
                                          ▼
                                  RenderParamsBuilder (每帧)
                                          │
                                          ▼
                                  渲染线程 ExportPipeline
                                          │
                                          ▼
                                  FfmpegEncoder (BGRA stdin)
```

### 3.2 公共抽象：`NoteProvider` trait

新建 `crates/export/src/video/note_provider.rs`：

```rust
/// 视频导出每帧的音符数据源。
///
/// 屏蔽“全内存文档”与“流式解析”两种实现差异。
pub trait NoteProvider {
    /// 收集指定 tick/key 范围内的可见音符实例。
    ///
    /// 调用方保证 `tick_start <= tick_end` 且单调递增调用（帧时间向前）。
    fn collect_instances(
        &mut self,
        tick_start: u32,
        tick_end: u32,
        key_min: u16,
        key_max: u16,
        color: [f32; 4],
        out: &mut Vec<lumino_gfx::NoteInstance>,
    ) -> Result<(), VideoExportError>;
}
```

> [PUA生效 🔥] 用 trait 抽象两种数据源，后续如果要支持 DMS / LMPJ / 实时录制等更多来源，直接实现 `NoteProvider` 即可，符合开闭原则。

---

## 四、模式 A：编辑器模式实现

### 4.1 职责

当 `RunnerInner::midi_state.current_midi` 存在时，直接复用 `MidiDocument`，避免任何重复解析。

### 4.2 实现文件

`crates/export/src/video/note_provider/document.rs`

### 4.3 核心逻辑

```rust
pub struct DocumentNoteProvider {
    document: Arc<MidiDocument>,
    /// 每个 track 当前查询起始索引，利用单调递增特性避免从头二分。
    track_cursors: Vec<usize>,
}

impl NoteProvider for DocumentNoteProvider {
    fn collect_instances(
        &mut self,
        tick_start: u32,
        tick_end: u32,
        key_min: u16,
        key_max: u16,
        color: [f32; 4],
        out: &mut Vec<NoteInstance>,
    ) -> Result<(), VideoExportError> {
        for (track_idx, notes) in self.document.notes.iter().enumerate() {
            let cursor = &mut self.track_cursors[track_idx];
            // 从 cursor 开始找到第一个 start_tick >= tick_start 的位置
            let start = notes[*cursor..]
                .partition_point(|n| n.start_tick < tick_start)
                + *cursor;
            // 向后扫描到 tick_end
            for n in &notes[start..] {
                if n.start_tick > tick_end {
                    break;
                }
                // 可见性判断：音符必须在 [tick_start, tick_end] 水平区间内，且 key 在范围内
                if n.end_tick >= tick_start
                    && n.key as u16 >= key_min
                    && n.key as u16 <= key_max
                {
                    out.push(NoteInstance::new(
                        n.start_tick as f32,
                        n.key as f32,
                        n.length() as f32,
                        color,
                    ));
                }
            }
            *cursor = start;
        }
        Ok(())
    }
}
```

### 4.4 优化点

- **单调游标**：视频导出帧时间单调向前，`track_cursors` 保证均摊 O(1) 定位，避免每帧二分。
- **颜色扩展**：后续可按 track/channel/velocity 映射颜色，当前先统一颜色。

---

## 五、模式 B：流式模式实现

### 5.1 目标

- 不加载完整 `MidiDocument`。
- 渲染过程中仅保留“当前帧可见 + 前方 lookahead”内的 active notes。
- 大文件（黑乐谱）也能低内存导出。

### 5.2 实现文件

`crates/export/src/video/note_provider/streaming.rs`

### 5.3 数据结构

```rust
/// 流式维护的单个 active note。
struct ActiveNote {
    start_tick: u32,
    end_tick: Option<u32>, // None 表示尚未收到 NoteOff
    key: u8,
    velocity: u8,
    channel: u8,
}

pub struct StreamingNoteProvider<'a> {
    player: StreamingMidiPlayer<'a>,
    tempo_map: TempoMap,
    /// 当前已解析到的最大 tick（lookahead 边界）。
    parsed_until: u32,
    /// 当前仍可能出现在屏幕上的音符。
    active_notes: Vec<ActiveNote>,
    /// 按 (channel, key) 索引未结束的音符，用于快速配对 NoteOff。
    pending_index: AHashMap<(u8, u8), Vec<usize>>,
}
```

### 5.4 核心逻辑

```rust
impl<'a> NoteProvider for StreamingNoteProvider<'a> {
    fn collect_instances(
        &mut self,
        tick_start: u32,
        tick_end: u32,
        key_min: u16,
        key_max: u16,
        color: [f32; 4],
        out: &mut Vec<NoteInstance>,
    ) -> Result<(), VideoExportError> {
        // 1. 推进流式解析器到 lookahead 边界
        //    lookahead = tick_end - tick_start（一屏的 tick 跨度）
        let lookahead = tick_end.saturating_sub(tick_start);
        let target = tick_end.saturating_add(lookahead);
        self.advance_to(target)?;

        // 2. 清理已经掉出屏幕的音符
        self.active_notes.retain(|n| {
            // 保留条件：未结束 或 结束 tick >= tick_start
            n.end_tick.map_or(true, |end| end >= tick_start)
        });

        // 3. 收集可见音符
        for n in &self.active_notes {
            if n.start_tick > tick_end {
                continue;
            }
            if n.key as u16 >= key_min && n.key as u16 <= key_max {
                let length = n.end_tick.unwrap_or(tick_end).saturating_sub(n.start_tick);
                out.push(NoteInstance::new(
                    n.start_tick as f32,
                    n.key as f32,
                    length as f32,
                    color,
                ));
            }
        }

        Ok(())
    }
}
```

### 5.5 事件处理

```rust
fn advance_to(&mut self, target_tick: u32) -> Result<(), VideoExportError> {
    while self.parsed_until < target_tick {
        let ev = match self.player.next_event() {
            Some(e) => e,
            None => break,
        };

        let tick = ev.0 as u32;
        self.parsed_until = tick;

        match ev.2 {
            TrackEventKind::Midi { channel, message } => match message {
                MidiMessage::NoteOn { key, vel } if u8::from(vel) > 0 => {
                    let idx = self.active_notes.len();
                    self.active_notes.push(ActiveNote {
                        start_tick: tick,
                        end_tick: None,
                        key: u8::from(key),
                        velocity: u8::from(vel),
                        channel: u8::from(channel),
                    });
                    self.pending_index
                        .entry((u8::from(channel), u8::from(key)))
                        .or_default()
                        .push(idx);
                }
                MidiMessage::NoteOn { key, vel } if u8::from(vel) == 0 => {
                    self.note_off(u8::from(channel), u8::from(key), tick);
                }
                MidiMessage::NoteOff { key, .. } => {
                    self.note_off(u8::from(channel), u8::from(key), tick);
                }
                _ => {} // CC/PC/PB 等可扩展
            },
            _ => {}
        }
    }
    Ok(())
}
```

### 5.6 与 Zenith-MIDI 的对照

| Zenith-MIDI | Lumino 流式视频 |
|-------------|-----------------|
| `BufferByteReader` 分块读文件 | `midly::mmap` 零拷贝 + `StreamingMidiPlayer` |
| `ParseUpTo(targetTime)` | `advance_to(target_tick)` |
| `globalDisplayNotes` 链表 | `active_notes: Vec<ActiveNote>` + `pending_index` |
| 渲染时遍历链表裁剪 | `retain` + 范围过滤后输出 `NoteInstance` |

---

## 六、视频导出 Runner 集成

### 6.1 入口逻辑

在 [`src/runner/menu/file/export.rs`](file:///d:/source/lumino-rs/src/runner/menu/file/export.rs) 新增 `handle_video_export_start`，与音频导出保持一致的分支：

```rust
pub(super) fn handle_video_export_start(
    &mut self,
    output_path: String,
    width: u32,
    height: u32,
    fps: u32,
    container: VideoContainer,
    codec: VideoCodec,
    backend: EncoderBackend,
    quality: QualityPreset,
    // ... 其他视频参数
) {
    let parsed_midi = self.midi_state.current_midi.clone();
    let midi_path_buf = std::path::PathBuf::from(&midi_path);
    let midi_on_disk = midi_path_buf.exists();

    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            if let Some(pm) = parsed_midi {
                // 模式 A：编辑器模式
                let document = pm.document.as_ref().ok_or_else(|| ...)?;
                let provider = DocumentNoteProvider::new(Arc::clone(document));
                render_video(provider, &config, ...)
            } else if midi_on_disk {
                // 模式 B：流式模式
                let bytes = std::fs::read(&midi_path_buf)?;
                let provider = StreamingNoteProvider::from_bytes(&bytes)?;
                render_video(provider, &config, ...)
            } else {
                Err(VideoExportError::NoMidiSource)
            }
        }).await;
        // ... 回调处理
    });
}
```

### 6.2 每帧渲染流程

```rust
fn render_video<P: NoteProvider>(
    mut provider: P,
    config: &VideoExportConfig,
    render_thread: &WgpuRenderThread,
    progress_cb: impl Fn(f32),
) -> VideoExportResult<()> {
    let total_seconds = ...; // 从 tempo_map 或 document 获取
    let total_frames = (total_seconds * config.fps as f64).ceil() as u64;

    // 发送开始导出命令到渲染线程
    render_thread.send_control(ControlCommand::StartVideoExport { width, height });

    for frame in 0..total_frames {
        let time_sec = frame as f64 / config.fps as f64;
        let tick = tempo_map.seconds_to_tick(time_sec) as f32;

        // 计算视口：跟随播放头
        let viewport_width_ticks = ...; // 由 zoom_x 决定
        let tick_start = tick;
        let tick_end = tick + viewport_width_ticks;
        let scroll_x = tick * zoom_x - fixed_indicator_px;

        // 收集音符
        let mut note_instances = Vec::new();
        provider.collect_instances(
            tick_start as u32,
            tick_end as u32,
            key_min,
            key_max,
            NOTE_COLOR,
            &mut note_instances,
        )?;

        // 构建 RenderParams
        let params = RenderParams::builder()
            .viewport_size((width, height))
            .logical_size((width as f32, height as f32))
            .scroll((scroll_x, scroll_y))
            .zoom((zoom_x, zoom_y))
            .note_instances(note_instances)
            .keyboard_width(...)
            .ruler_height(...)
            .build();

        // 发送渲染命令并等待读回
        render_thread.send_control(ControlCommand::RenderVideoFrame(Box::new(params)));
        let bgra = render_thread.receive_video_frame()?; // 需要新增 API

        // 写入 FFmpeg
        encoder.write_frame(bgra)?;

        progress_cb(frame as f32 / total_frames as f32);
    }

    render_thread.send_control(ControlCommand::FinishVideoExport);
    encoder.finish()?;
    Ok(())
}
```

---

## 七、新增/修改文件清单

| 文件 | 新增/修改 | 说明 |
|------|----------|------|
| `crates/export/src/video.rs` | 新增 | 视频导出模块入口 |
| `crates/export/src/video/config.rs` | 新增 | 视频导出配置 |
| `crates/export/src/video/ffmpeg.rs` | 新增 | FFmpeg 编码器（移植 Nezha） |
| `crates/export/src/video/error.rs` | 新增 | 错误类型 |
| `crates/export/src/video/note_provider.rs` | 新增 | `NoteProvider` trait |
| `crates/export/src/video/note_provider/document.rs` | 新增 | 模式 A 实现 |
| `crates/export/src/video/note_provider/streaming.rs` | 新增 | 模式 B 实现 |
| `crates/export/src/video/frame_renderer.rs` | 新增 | 帧生成器（scroll 计算 + RenderParams 构建） |
| `crates/gfx/src/render_thread/export_pipeline.rs` | 新增 | GPU→CPU 读回管线 |
| `crates/gfx/src/render_thread/commands.rs` | 修改 | 新增视频导出控制命令 |
| `crates/gfx/src/render_thread/render_loop/runner/run.rs` | 修改 | 处理视频导出命令 |
| `crates/gfx/src/render_thread/thread.rs` | 修改 | 暴露读回结果 API |
| `src/runner/menu/file/export.rs` | 修改 | 新增 `handle_video_export_start` |
| `crates/event/src/menu/file.rs` | 修改 | 新增 `VideoExportStart` |
| `crates/message/src/video_export.rs` | 新增 | 视频导出消息动作 |
| `crates/ui/src/view/video_export_dialog.rs` | 新增 | 视频导出面板 |
| `crates/ui/src/state/root_state.rs` | 修改 | 新增视频导出状态 |
| `crates/ui/src/sidebar/route.rs` | 修改 | 激活 `VideoExport` 路由 |

---

## 八、可执行路线图

| 阶段 | 任务 | 验收标准 |
|------|------|----------|
| **P1** | 实现 `NoteProvider` trait + `DocumentNoteProvider` | 单元测试：给定 MidiDocument 能正确收集帧音符 |
| **P2** | 实现 `StreamingNoteProvider` | 单元测试：与 `DocumentNoteProvider` 输出一致（同文件、同视口） |
| **P3** | 集成渲染线程读回 + FFmpeg | 端到端导出 1080p30 视频 |
| **P4** | UI 面板 + Runner 事件接线 | 侧边栏点击「视频渲染」可配置并导出 |
| **P5** | `cargo fmt` + `cargo clippy --all-targets` | 零警告 |

---

## 九、关键风险与对策

1. **流式模式 NoteOff 配对**：MIDI 中可能出现多个同 key 的 NoteOn 未收到 NoteOff。对策：用 `VecDeque` 存储 pending notes，NoteOff 时 pop 最早的一个。
2. **超大 lookahead 内存爆炸**：若 `viewport_width_ticks` 极大（整首乐曲一屏显示），流式模式会保留大量 active notes。对策：面板限制最大可见时间跨度，或降级为模式 A。
3. **渲染线程阻塞**：视频导出独占渲染线程期间，UI 无法更新。对策：导出期间禁用交互；未来可考虑独立 wgpu 实例。
4. **颜色一致性**：两种模式输出颜色需一致。对策：统一颜色生成器，按 track/channel 映射。

---

## 十、下一步

方案已对齐。需要我现在开始实现 **P1**（`NoteProvider` trait + `DocumentNoteProvider`）吗？我会先写单元测试，再写实现，保证两种模式输出等价。

---

