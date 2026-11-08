# egui UI 层重写执行计划

> 状态：执行中
> 分支：`feat/wgpu-upgrade`
> 决策人：用户 / 主 Agent
> 最后更新：2026-11-06

## 一、已确认决策

1. **草稿阶段 GPU 组合**：`egui 0.37 + wgpu 30` 独立 example，不碰现有 `wgpu 27` 主程序。
2. **重构阶段时序**：先做 `egui-wgpu 0.37 → wgpu 27` 桥接，保持主程序可运行；UI 替换完成后再把 `lumino-gfx` 等迁到 `wgpu 30`，切回官方 `egui-wgpu`。
3. **首批视觉基线**：主窗口 Chrome + 设置页 + 一个对话框 + 钢琴卷帘骨架。
4. **Agent 并行策略**：
   - 单个 Agent 的任务：允许 Agent 自行运行 `cargo check/fmt/clippy/test`。
   - 多个 Agent 并行的任务：**禁止任何 Agent 运行编译/测试/格式化**，由主 Agent 统一编译与验收。
5. **视觉验收标准**：
   - 几何严格：布局、控件矩形、边框、像素边界一致。
   - 颜色容差：非文字区域通道差 ≤ 2（或 ΔE ≤ 2）。
   - 文字区域：截图 diff + 人工复核，不追求逐像素一致。
6. **GPU 验收**：所有 wgpu 独立渲染画面在迁移前后必须通过基线图对比；无损坏、无外观变化。

## 二、不可突破的边界

- 不改 `lumino-editor-state` 的文档/选区/历史模型。
- 不改 `lumino-message` 的处理器语义；只允许把 `iced_widget::text_editor::Action` 换成中性枚举。
- 不改 `lumino-gfx` 的音符场渲染语义；wgpu30 迁移只做 API 适配。
- 不移除现有测试；只新增 UI 截图/GPU 基线测试。
- 不在主分支直接替换默认 UI；所有重构在独立 crate / 独立分支完成，最后一次性切换。

### 执行发现（2026-11-07）

主程序的 `iced_wgpu::Engine` 与 `lumino-gfx` 共用**同一个 `wgpu::Device/Queue`**
（`crates/editor/ui/src/host/render_ctx.rs::WgpuResources`），因此 wgpu 版本被硬绑定：
iced 侧停留在 27 时，gfx 无法单独升到 30。这确认了「先做 27 桥接、后整体升 30」
的时序是必要的；「先把 gfx 升到 30、保留 iced」在 iced 仍参与渲染时不可行。

桥接实现建议（比整体移植 egui-wgpu 小一个量级）：只实现「egui 网格 → wgpu27 绘制」的
最小 painter——顶点/索引缓冲、字体图集纹理、scissor、sRGB 混合；不移植 egui-wgpu 的
callback / 多视口 / 截图能力。

## 三、验收工具链

### 3.1 iced 基线

- 使用 `iced_test 0.14` 的 `Simulator::with_size` + `snapshot` / `matches_image`。
- 固定场景：
  - `main_chrome`：1600×1000，TokyoNightStorm，zh-CN，无光标。
  - `settings_general`：1200×800，设置页选中「常规」。
  - `save_confirm_dialog`：560×260，保存确认对话框。
  - `piano_roll_skeleton`：1600×1000，空工程 + 钢琴卷帘骨架。
- DPI：1.0 / 1.25 / 1.5 三档。
- 输出：`tests/ui-baseline/iced/{scene}@{dpi}.png`。

### 3.2 egui 草稿

- 使用 `egui 0.37` + `egui_kittest 0.37`（`wgpu` feature）或 `Context::request_screenshot`。
- 同一 `SceneSpec`、同一尺寸、同一字体/主题 token。
- 输出：`tests/ui-baseline/egui/{scene}@{dpi}.png`。

### 3.3 对比

- `ui-diff` 工具：
  - 尺寸不一致：直接失败。
  - 几何：背景掩码 + 边缘图，允许 1px 膨胀容差。
  - 颜色：非文字区域逐像素通道差，容差 2。
  - 输出 `*.diff.png` 与 JSON 报告。
- 人工复核：文字、图标、抗锯齿、阴影等区域。

### 3.4 GPU 基线

- 复用 `crates/editor/gfx/tests` 的离屏渲染 + 回读设施。
- 覆盖：钢琴卷帘横向/纵向、洋葱皮 GPU/CPU、走带、力度/CC、贴图瀑布流、MIDITrail、视频导出、设备重连前后。
- 输出：`tests/gpu-baseline/{scene}.png`。
- wgpu30 迁移前后逐像素对比；允许硬件后端舍入差 1 级通道值。

## 四、任务分解

### Phase 0：基线与 UI 草稿

| 编号 | 任务 | Agent | 编译权 | 交付 |
|---|---|---|---|---|
| 0.1 | 视觉基线工具链（iced 侧）+ 场景清单 | 1 个 | 允许 | `lumino-ui-parity` crate、4 张 iced 基线图、diff 工具 |
| 0.2 | egui 草稿骨架 + 截图 | 1 个 | 允许 | `ui-parity` 的 egui backend、4 张 egui 草稿图 |
| 0.3 | 主窗口 Chrome 移植与对齐 | 1 个 | 允许 | `lumino-ui-egui` chrome 模块、对比通过 |
| 0.4 | 设置页 + 对话框移植与对齐 | 2 个并行 | **禁止** | 设置页框架 + 保存确认/设置对话框 |
| 0.5 | 钢琴卷帘骨架移植与对齐 | 1 个 | 允许 | painter overlay、命中/滚动骨架 |
| 0.6 | GPU 基线捕获 | 1 个 | 允许 | 8+ 张 GPU 基线图、对比脚本 |

### Phase 1：wgpu30 迁移

| 编号 | 任务 | Agent | 编译权 | 交付 |
|---|---|---|---|---|
| 1.1 | `egui-wgpu 0.37 → wgpu27` 桥接 crate | 1 个 | 允许 | 临时 `lumino-egui-wgpu27`，主程序可叠 egui |
| 1.2 | `lumino-gfx` wgpu30 机械迁移 | 1 个 | 允许 | API 迁移、GPU 基线通过 |
| 1.3 | `gpu-synth` / `midiplayer` / `ui` keyboard renderer wgpu30 | 1 个 | 允许 | 全部 wgpu 用户统一到 30 |
| 1.4 | 切回官方 `egui-wgpu 0.37`，删除桥接 | 1 个 | 允许 | 单一 wgpu30、官方 renderer |

### Phase 2：UI 层全量替换

| 编号 | 任务 | Agent | 编译权 | 交付 |
|---|---|---|---|---|
| 2.1 | `lumino-ui-egui-core`：主题/字体/图标/控件/消息队列 | 1 个 | 允许 | 可复用 UI 基础层 |
| 2.2 | Chrome 全量：标题栏/菜单/工具栏/左右栏/状态栏/Toast | 2 个并行 | **禁止** | 与 iced 视觉一致 |
| 2.3 | 设置 + 21 类对话框 | 2 个并行 | **禁止** | 10 页设置、全部对话框 |
| 2.4 | 钢琴卷帘绘制 + 交互 + 工具栏 + 力度 | 2 个并行 | **禁止** | 全工具 parity |
| 2.5 | 走带/瀑布流/视频剪辑/协作/云 | 2 个并行 | **禁止** | 全面板 parity |
| 2.6 | Android/macOS/Windows/Linux 平台层 | 1 个 | 允许 | IME/DPI/原生菜单 |
| 2.7 | 移除 iced、清理、文档、全量测试 | 1 个 | 允许 | 默认 egui UI，iced 依赖删除 |

## 五、每个任务的验收流程

1. 主 Agent 读取 Agent 工作报告与 `git diff`。
2. 检查是否越界修改、是否偷工减料、是否新增 `unwrap/expect`。
3. 单 Agent 任务：运行 `cargo check -p <crate>`、`cargo fmt --check`、`cargo clippy -p <crate> --all-targets -- -D warnings`、相关测试。
4. 多 Agent 并行任务：主 Agent 统一编译/格式化/测试。
5. 运行 `ui-diff` / GPU 基线对比。
6. 人工复核文字/图标区域。
7. 合格 → 提交/进入下一任务；不合格 → 重派，同一 Agent 重派不超过 2 次。

## 六、回滚策略

- 每个任务独立提交；UI 草稿与生产代码隔离。
- Phase 1 桥接 crate 独立，可随时删除。
- wgpu30 迁移保留 `wgpu27` 分支/标签；GPU 基线不通过则回滚该迁移提交。
- 最终切换前保留 iced UI crate，不删除旧代码。

## 七、执行记录

### Task 0.1：视觉基线工具链（iced 侧）— 已完成

- 分支 `feat/egui-ui-ref`，提交 `902df0d1 feat(ui): 引入 UI 一致性校验工具链与重构计划`。
- 交付：`crates/editor/ui-parity`（`capture_iced` / `ui-diff` / `scene` / `diff`）、
  `tests/ui-baseline/iced/` 12 张基线图（4 场景 × 3 DPI）。

### Task 0.2：egui 草稿骨架 + 截图 — 已完成

- **Sub-agent 版本已回退**：其实现靠手调像素、图标是手绘近似、未使用 `egui_extras`
  网格，与验收要求不符。本任务由主 Agent 重写。
- 交付：`crates/editor/ui-egui`
  - 真实 SVG 图标（`resources/icons/**` 原图 + 生产尺寸 + 暗色染色规则）；
  - 生产调色板精确值（由 `dump_palette` 从 iced `Extended` 导出，非采样近似）；
  - 主窗口 Chrome（标题栏 / 工具栏分组 / 左右路由栏 / 音轨列表 / 状态栏 / 钢琴卷帘骨架）；
  - 设置页（`egui_extras::TableBuilder` 菜单 + 表单双列）、保存确认对话框；
  - `capture_egui` / `compare_egui` 两个可执行工具。
- 布局事实（来自 iced 源码 + 基线像素双向核对）：
  - 根结构 `column![titlebar, row![left_bar, column![toolbar, row![editor, right_bar]]], statusbar]`；
  - 工具栏分组：50px 高、圆角 4、`background.weak`；分组间距 4/8/8/16/16/0；
  - 左侧栏 `48 + 200`，面板内边距 8，标题 12px，行高 34 + 行距 2；
  - 滚动条厚度 20（`V_SCROLLBAR_WIDTH` / `H_SCROLLBAR_HEIGHT`）；
  - 路由栏激活指示条 2px，父组用 `parent_color`、当前路由用 `child_color`。
- 视觉对比（`compare_egui`，容差 2）：

  | 场景 | 超容差像素占比 | 平均通道差 |
  |---|---|---|
  | main_chrome | 3.0% – 3.6% | 1.66 – 1.75 |
  | settings_general | 3.4% – 4.0% | 1.49 – 1.65 |
  | save_confirm_dialog | 3.2% – 3.9% | 2.63 – 2.91 |
  | piano_roll_skeleton | 2.9% – 3.6% | 1.57 – 1.70 |

  残差集中在字形栅格化（egui 字体栈 vs iced cosmic-text）与图标抗锯齿，几何/色块已对齐。
- 质量门：`cargo check` / `cargo clippy -D warnings` / `cargo test` 全绿；所有源文件 < 400 行。

### Task 0.6：GPU 基线捕获 — 部分完成（v1）

- 新增 `crates/editor/gfx/tests/gpu_baseline`（复用 `cpu_gpu_parity` 的离屏设备与回读）：
  - `note_renderer`：音符实例主路径（洋葱皮 / indirect draw）；
  - `grid_renderer`：无限网格（背景 / 黑键区 / 小节 / 拍 / 细分线）；
  - `ruler_renderer`：时间标尺刻度。
- 基线产物：`tests/gpu-baseline/{scene}.png`；逐像素比较，容差 ±1 通道。
- **未覆盖**（需要产品级场景夹具，Phase 1.2 迁移前补齐）：
  走带 `arrangement`、力度 `cc_bar`、`midiconsole`、`miditrail`、瀑布流、视频导出。
  接入点已留在 `tests/gpu_baseline/main.rs`。

### Task 1.1：`egui-wgpu 0.37 → wgpu27` 最小绘制桥 — 已完成

- 新增 `crates/editor/egui-bridge`（package `lumino-egui-bridge`，依赖 `egui 0.37` + `wgpu 27`）。
- 实现范围（刻意不移植整个 `egui-wgpu`，2900 行 → 约 520 行）：
  - `TexturesDelta` 全量 / 分块上传，纹理格式 `Rgba8Unorm`（与 egui-wgpu 一致）；
  - `ClippedPrimitive` 顶点 / 索引缓冲、scissor、预乘 alpha 混合；
  - gamma / sRGB 双片元入口，按目标格式选择（同 egui-wgpu）；
  - 与 `RendererOptions::PREDICTABLE` 相同的四抽头双线性 + ClampToEdge 采样；
  - scissor 的四边 `round` + `clamp` 规则逐行对齐 `ScissorRect::new`。
- 验收：
  - `crates/editor/egui-bridge/tests/painter.rs`：纯色 sRGB 往返、scissor 裁剪、字体图集文字三组 GPU 测试；
  - `crates/editor/ui-egui/tests/bridge_parity.rs`：4 个场景（main_chrome / settings / dialog /
    piano_roll）用桥接渲染，与 egui-kittest 基线逐像素对比：

    | 场景 | 超容差像素 | 最大通道差 | 平均通道差 |
    |---|---|---|---|
    | main_chrome | 0 | 1 | 0.002 |
    | settings_general | 0 | 2 | 0.004 |
    | save_confirm_dialog | 0 | 1 | 0.008 |
    | piano_roll_skeleton | 0 | 1 | 0.003 |

- 关键发现：`egui_kittest` 无表面时目标格式是 `Rgba8Unorm`（gamma 路径）；生产 UI 通道是
  `Rgba8UnormSrgb`。桥接两条路径都已实现，对比测试必须用 gamma 目标才能与基线逐字节对齐。
- 已知限制（迁移到 wgpu30 后由官方 `egui-wgpu` 覆盖）：不支持 `PaintCallback`、多视口、
  nearest / repeat 采样；顶点/索引缓冲按帧重建，暂未做容量复用。

### Task 1.2（第一步）：真实窗口叠加验证 — 已完成

- 新增 `crates/editor/ui/src/host/render/egui_overlay.rs`，由 `LUMINO_EGUI_OVERLAY=1`
  开关（默认关闭）。在 iced `present` 之后、`with_frame` 的 `frame.present()` 之前绘制，
  Device/Queue 与 iced 完全共用。
- 实测环境证据（Windows / Vulkan，窗口 2560×1540 @1.25）：
  - surface 格式为 `Bgra8UnormSrgb` → 走 sRGB 片元入口；
  - 叠加层首帧 `primitives=1 drawn=1`，stderr 无 wgpu 校验错误；
  - 截图中右上角 HUD（`egui overlay · wgpu27` / `Bgra8UnormSrgb` / `2560×1540`）正常显示。
- 为生产格式补测试：`lumino-egui-bridge` 新增 `Bgra8UnormSrgb` 半透明混合用例，
  同时验证 sRGB 目标线性混合（≈135）与 gamma 目标直接混合（≈160）。
- 截图注意：PowerShell 截屏前必须调用 `SetProcessDPIAware()`，否则 ppp>1 时只截到
  窗口左侧（右半部分被裁掉），会误判「叠加层没显示」。窗口被 `ShowWindow(SW_RESTORE)`
  还原后不再是全屏，必须用 `SW_MAXIMIZE` 并校验 client 尺寸。

### Task 1.3（第一步）：真实状态接管标题栏 + 状态栏 — 已完成

- 新增只读视图模型 `lumino-ui-egui::chrome::{ChromeModel, TitlebarModel, StatusbarModel}`：
  `lumino-ui-egui` 不依赖 iced，生产侧 `Root::egui_chrome_model()`（`root/view/egui_chrome.rs`）
  负责把真实状态填进模型。
- 状态栏快照 `StatusBar::egui_snapshot()` 与生产 `view()` 共用同一套左文本规则
  （悬停说明 / 状态消息 / 音符数 / 就绪）与 FPS 高亮判定，避免两套规则漂移。
- `chrome::show_bands()` 只画两条不透明 band（标题栏 30 / 状态栏 20），其余区域
  完全不绘制，因此下层 iced 画面保持可见，输入仍全部由 iced 处理。
- 关键几何修正（以 iced 基线实测为准）：
  - 菜单项宽度 = 文本宽 + 17（中文 14px 下 45px 节距），字号 14；
  - 状态栏右组 label→value 间距 1、组间 14；`right_text` 即使为空也占 8px 行位
    （此前少 8px，导致整组右移）；
  - 叠加层设置 `ctx.set_pixels_per_point(真实 DPI)`，否则字体图集按 1× 生成再被
    着色器放大，几何正确但字形发虚；
  - `egui_kittest` 会给根 Ui 加 8px 外缩进，`show_bands` 内部按 `viewport_rect`
    重建根 Ui（生产接线时为无操作）。
- 验收证据：
  - `tests/bands_pixels.rs`：1600×1000 @1x，与 iced 基线逐列比较墨迹区间——
    标题栏（logo + 四菜单）、状态栏左侧、状态栏三组指标全部通过（容差 4px，
    吸收 cosmic-text / egui 的字形宽度差异）；band 底色逐字节一致。
  - `tests/chrome_bands.rs`：模型 → 可访问节点（菜单 / 指标 / 空 FPS 隐藏指标组 /
    原生标题栏跳过顶部 band）。
  - 真实窗口（2560×1540 @1.25，`Bgra8UnormSrgb`）：标题栏菜单起点
    `21..38 / 70..105 / 126..161 / 183..217 / 239..273`（1.25× 基线 ±2px），
    状态栏右侧墨迹止于 2539（≤ 内容右界 2540），首帧 `primitives=7`。

### Task 1.3（第一步补充）：场景错位 / 重叠修复 — 已完成

- 以 iced 源码（`grid/keyboard.rs`、`grid/playback_indicator.rs`、
  `scrollbar_widget`、`toolbar/buttons.rs`、`ui-settings`）与基线像素为准逐项对齐。
- 钢琴卷帘：
  - 音名居中于键盘宽度、纯白（原 `center-12` + `TEXT_MUTED`）；
  - 键位描边改 `BASE_BG`（原 `BORDER` 过亮）；播放线中心对齐 + 8×8 倒三角 + `#FF3333`；
  - 布局改为 `column![row![canvas, vscroll], hscroll]`，水平滚动条横跨全宽；
    滚动条轨道 `SELECTED_BG`、滑块 `TOOLBAR_BG`、2px 内缩、按比例定位。
- 工具栏：精度下拉 57×26（`TOOLBAR_BG` 底 + `SELECTED_BG` 边框、16 图标 + 13 文本、
  标签 14 且用 `TEXT_ON_WEAK`）；工具间距按 `ToolItem::gap_before`（默认 4、
  Split/TransposeDown/Tie = 8）；录制红点 14→8。
- 设置页：`pick_list` 32 高 / 内边距 8 / 字号 16；`text_input` 80×31、
  `BASE_BG` 底 + `SELECTED_BG` 边框；日志行改为「标签宽 + 32 间距」消除与输入框重叠；
  菜单项按「容器 8 + 项 16 + 图标容器 24 + 间距 8 + 箭头 12」重排。
- 对话框：按钮强制 90×35（`ui.put` 会收缩内容），主按钮边框 `ACCENT_DARK`、
  次按钮 `ROUTE_BAR_BG`。
- 验收（`compare_egui`，dpi100 over_tol / mean）：
  `main_chrome` 2.99%/1.59 → **1.65%/1.07**；`piano_roll` 2.92%/1.52 → **1.80%/1.16**；
  `settings_general` 3.42%/1.49 → **2.98%/1.02**；`save_confirm_dialog` 3.39%/2.63 → **2.88%/2.31**。
  差异图确认无结构性错位，残余为字形 1px 重影与 0.5px 分数布局。

## 八、当前任务

**Task 1.3（第二步，第一步）：工具栏 band 接管 — 已完成（只读）**

- 新增 `lumino-ui-egui::chrome::ToolbarModel`，生产侧 `Toolbar::egui_snapshot()` 填充：
  - 文本：BPM、`PPQ {ppq}`、小节.拍、`mm:ss.cs`、吸附规格；
  - 状态：当前工具、绘制面板开关、播放/循环/录制、洋葱皮、自动滚动模式。
- 时间码与 iced 仪表盘共用 `bpm_at_tick` / `tick_to_seconds` / `format_position` /
  `format_time`，不复制算法。
- `show_bands` 在左侧栏右缘（`Sidebar::width()`）画 72px 不透明 band，覆盖 iced 工具栏；
  不喂指针事件，输入仍由 iced 处理。
- 验证：
  - `bands_pixels` 新增工具栏断言：band 左界 248（247 非 PANEL_BG / 248 为 PANEL_BG），
    y 38..88、x 248..1599 的墨迹区间与 iced 基线逐段一致；
  - 真实窗口（2560×1540 @1.25）：首帧 `primitives=39 drawn=38`，截图显示真实
    `120.0 / PPQ 1920 / 1.1 / 00:00.00 / 1/4` 且无重影；
  - 四个场景 parity 与上一提交完全一致（重构无行为变化）。

**Task 1.3（第二步，第二步）：左侧栏接管 — 已完成（只读）**

- 新增 `lumino-ui-egui::chrome::{SidebarModel, SidebarTrack, SidebarGroup, SidebarPanel, RollBar}`，
  生产侧 `Sidebar::egui_model()` 填充：路由栏状态（分组激活 / 子面板可见 / 卷帘方向 / 混音台）
  与音轨列表（编号、名称、颜色、选中、指挥轨、独奏）。
- `show_bands` 在 x 0..`48 + panel_width`、y 标题栏下沿..状态栏上沿绘制不透明 band，
  不覆盖标题栏 / 状态栏，也不消费输入。
- 瞬态守卫：重命名、拖拽排序激活、音轨/面板右键菜单、颜色/端口/通道选择器、宽度拖拽
  时 `sidebar = None`，整块交回 iced；拖拽候选（未激活）仍接管。
- 几何修正（以 iced 源码 + 基线实测为准）：
  - 音轨行高 35 → **36**（14px 文本行高 18 + 行内边距 8 + 按钮内边距 10）；
  - 滚动条**不占布局宽度**，行宽 = 面板内容宽 - 16（此前多减 8px，行右侧短 8px）；
  - 指挥轨图标列 24 宽（名称起点比普通轨早 12px），恢复 iced 原始列布局；
  - 路由栏按钮 48×48、图标 x = 左 + 14、灯条 2px，子按钮仅在分组激活时出现。
- 验收：
  - `bands_pixels` 覆盖左侧栏：路由栏底色、band 上下沿、选中行底色、行右边界、
    路由栏图标墨迹、添加音轨行墨迹，均与 iced 基线一致；
  - `compare_egui`：`main_chrome` 1.65%/1.07 → **1.56%/0.95**，
    `piano_roll_skeleton` 1.80%/1.16 → **1.71%/1.04**；
  - 真实窗口（2560×1540 @1.25）首帧 `primitives=58 drawn=57`，截图显示真实音轨与路由状态。

**Task 1.3（第四步）：右侧栏 + 钢琴卷帘骨架 — 右侧栏已完成，卷帘骨架默认不接管**

- 右侧栏 48px 图标列已由 egui 用真实状态绘制：
  - 按钮 48×48、**间距 2px**、图标 20×20 在 46px 内容区居中（起点 = 左 + 13）、
    激活灯条贴右边缘 2px；band 位于 `width-48 .. width`、y = 标题栏 + 工具栏 .. 状态栏。
  - 修正旧场景的 0 间距（第 2/3 个图标分别高 2px / 4px）与图标右偏 1px；
    kittest 图标行与 iced 基线逐行一致（117..134 / 169..182 / 216..235）。
  - 面板（素材库 / I2M / 瀑布流）仍在 iced：内容含扫描、文件对话框等复杂交互。
- 钢琴卷帘骨架模型已按真实视图状态重建（`keyboard_width` / `ruler_height` /
  `zoom_x` / `scroll_x` / `zoom_y` / `scroll_y` / `visible_keys` / `ticks_per_measure` /
  `playhead_offset` + 滚动条比例 + 四边内缩），并修正：
  - 琴键 1px 分隔线按 iced 的**居中描边合成结果**（50% `background.base`）绘制；
  - 垂直滚动条滑块 `278/874` → `279/854`（此前短 7px）；
  - 标尺小节号与键盘改为按缩放/滚动计算（场景与 iced `PianoRollSkeleton` 同源）。
- **生产默认不接管卷帘骨架**：关掉叠加层的真实窗口截图显示，卷帘的键盘 / 标尺 /
  网格整体由 `lumino-gfx` 的 wgpu 通道绘制（`grid_renderer`，含键色与标尺实例），
  iced canvas 的同名几何被其覆盖。egui 骨架直接接管会把用户的浅色网格盖成 iced
  深色键盘，属视觉回归；因此 `ChromeModel.piano_roll` 默认恒为 `None`，
  `LUMINO_EGUI_PIANOROLL=1` 可实验性启用，并有单测锁定「默认关闭」。
- 验收：
  - `compare_egui`（dpi100）：`main_chrome` 1.56%/0.95 → **1.23%/0.96**，
    `piano_roll_skeleton` 1.71%/1.04 → **1.19%/0.89**；
  - `bands_pixels` 新增右侧栏 band 边界 + 三个图标行、卷帘键盘首键音名 / 标尺小节号 /
    键色与 50% 分隔线 / 两条滚动条滑块，以及「网格区域 alpha 必须为 0」；
  - 真实窗口（2560×1540 @1.25）首帧 `primitives=61 drawn=60`；右侧栏 band
    `(2499,300)` 为网格色、`(2500,300)=#333957`、上下沿与状态栏/工具栏对齐；
    卷帘区域像素与无叠加层基线完全一致。

**Task 1.3（第五步）：对话框整窗接管 + 让位 iced 弹出层 — 已完成**

- 修复两个真实缺陷：
  - 叠加层在**所有** Host 窗口（对话框/进度窗/警告窗）上运行，此前会把主窗口 band
    画到对话框上；现在非主窗口整块返回空模型（真实窗口设置对话框首帧 `drawn=0`）。
  - iced 菜单栏下拉 / 工具栏下拉 / 工具面板画在叠加层**下方**，此前被不透明 band
    盖住（能点但看不见）。现在 `is_menu_open` + 8 个工具栏弹出开关任一打开即整块让位；
    菜单栏不可探测，改为**标题栏按下闩锁**（标题栏内按下即让位，下一次别处按下恢复）。
    真实窗口点「文件」菜单完整可见。
- 保存确认对话框整窗接管：
  - `ChromeModel.dialog` + `DialogModel::SaveConfirm(SaveConfirmModel)`；
    `dialog::show_model` 复刻生产几何（标题栏 30 / 提示语 16px +90 / 按钮 90×35 间距 10 顶边 +126），
    文案由生产侧 `tr!` 填充；**浅色主题不接管**（本层仅有深色调色板）。
  - 修正场景字号错误：提示语 18 → **16**，`save_confirm_dialog` parity 2.88%/2.31 → **2.75%/2.19**。
- 验收：
  - 新增 `dialog_bands`：560×260 @1x 的 `show_bands` 整窗对比 iced 基线（按钮位置/底色、
    提示语墨迹、整窗差异率 < 5%，实测 2.75%）；
  - `lumino-ui` 单测 13 个（对话框不接收主窗口 band、保存确认文案、浅色主题让位、
    进度/警告窗、菜单/下拉/闩锁让位与恢复）；
  - `capture_egui` 12/12；`cargo test`、`clippy -D warnings`、`check --bin lumino-rs` 全绿；
  - 真实窗口（2560×1540 @1.25）：菜单让位可见、设置对话框 `drawn=0`、添加音轨点击后
    左侧栏即时出现新轨道（输入 → 状态 → egui band 闭环）。

**Task 1.3（第五步）：设置对话框调色板注入 + 浅色基线 — 已完成**

- `SettingsPalette`（11 角色）由 `Root::settings_palette()` 从 iced `extended_palette`
  注入，`settings::show_with` 全量使用；`widgets::combo_box` / `numeric_input` 也接收
  调色板（下拉底色 `background.weak`、输入框 `background.base` + `background.strong` 边框）。
- 移除设置页浅色闸门：浅色主题下设置对话框整窗接管。
- `ui-parity` 新增 `SettingsGeneralLight` 场景（`Theme::Light`），`capture_iced` 产出
  3 张浅色 iced 基线（15/15 校验通过）。
- 验收：`dialog_bands` 3/3（深色保存确认 2.75%、深色设置 <5%、**浅色设置表面颜色与
  浅色基线逐点一致 + 菜单墨迹一致**）；`egui_chrome` 14/14；`ui-parity` 12/12；
  真机浅色设置对话框 `primitives=28 drawn=28`（此前 drawn=0），截图确认浅色下拉/输入框。

**Task 1.3（第六步）：设置对话框本地化 + 表单错位修复 — 已完成**

- 文案不再写死在渲染层：`SettingsLabels` 由 `Root::settings_labels(language)` 从生产
  `settings_translations` / `eraser_behavior_name` / `track_add_behavior_name` 填充，
  中英文共用一条链路；移除中文闸门（仍只接管「常规」页）。
- **错位修复**：表单控件列由固定 104px 改为「标签实测宽度 + 32」
  （生产 `row![label, space(16), pick_list].spacing(8)`），英文 `Add Track Behavior:`
  不再被下拉框压住。
- **数据修复**：日志保留份数改读 `logging.log_retention_count`（原误读历史条数）。
- 设置菜单行高 48 → **52**（iced 基线逐行 52）。
- 菜单滚动让位：叠加层不消费滚轮、无法复刻 iced `scrollable`；窗口逻辑高度
  < 672 时整块交回 iced（默认 900×675 对话框 `drawn=0`），高度足够才整窗接管。
- 验收：
  - `settings_bands` 3/3（深色 <5%、浅色表面色逐点一致、英文菜单/橡皮擦/日志行
    墨迹与英文基线一致）；
  - `SceneId::SettingsGeneralEn` + 3 张英文 iced 基线（`capture_iced` 18/18）；
  - 真实窗口 `LUMINO_LANG=en-US`：默认尺寸 `drawn=0`（让位）；放大到 1500×1000 后
    与 overlay-off 同尺寸截图差异 4.19%，截图 10 个英文菜单项齐全、表单无重叠。

**Task 1.3（第七步）：叠加层输入路由 + 设置菜单真滚动 — 已完成**

- 叠加层不再是「零输入」：`OverlayInput` 收集指针移动 / 滚轮 / 修饰键 / 焦点，
  由 `Host::capture_egui_overlay_input` 在 `handle_events` 里同步（**只观察不消费**，
  动作仍由 iced 执行；不回填鼠标按键，避免出现「按下却没反应」的假态）。
- 设置菜单放不下时改用 `egui::ScrollArea`（8px 实体滚动条，对齐 iced `scrollable`）；
  放得下时仍走原 `TableBuilder` 路径，既有像素基线逐点不变。
- **删除「菜单放不下就整窗交回 iced」的闸门**：默认 720×540 逻辑点也整窗接管，
  滚轮可滚到第 10 项。`Root::egui_chrome_model_for(viewport)` 随之删除。
- 验收：
  - `settings_bands` 4/4：新增 `settings_menu_scrolls_to_last_item_on_small_window`
    （720×540；注入 PointerMoved + MouseWheel 后首项滚出、末项「关于」可见）；
  - 真实窗口（`LUMINO_LANG=en-US`，720×540 逻辑点）：叠加层首帧
    `primitives=28 drawn=28`（原 `drawn=0`）；向对话框投递 `WM_MOUSEMOVE` +
    `WM_MOUSEWHEEL` 后截图显示菜单从 General 滚到 About，滚动条到底；
  - `clippy -D warnings` / `check --bin lumino-rs` / `capture_iced` 18/18 /
    `capture_egui` 12/12 / `compare_egui` 与上一步完全一致。

**Task 1.3（第八步）：菜单栏迁移 + 点击路由 — 已完成**

- 生产菜单树镜像进 egui：`titlebar::menu::menus()` →
  `MenuBarModel / MenuModel / MenuEntry`（动作只带整棵树的遍历序号），
  点击写回序号，`Root::menu_action_message(index)` 再解析回 `Message` 交给
  `process_message`。模型与解析共用同一遍历顺序，单测覆盖全部 25 个动作。
- 下拉面板画在整窗 root 末尾（不再被标题栏 30px 裁剪）；面板宽 200、菜单项 28、
  分隔线 9、标签偏移 +9，与 iced_aw 几何一致。一级子菜单右贴父行，并**持久化
  展开状态**：指针从父行斜向移入子面板不会闪断（本轮修掉的真实交互缺陷）。
- 输入路由：`OverlayInput.pointer` 记录指针位置后回填鼠标按键；叠加层每帧发布
  菜单占用矩形（标签 + 下拉 + 子面板），`Host::egui_blocked` 命中时
  `handle_events` 提前返回——点击只给 egui，不再触发下层工具栏/侧栏。
  自绘标题栏模式下 iced 菜单换成 `menu::view_placeholder`（同宽、零输入）；
  系统原生标题栏仍保留 iced 菜单与整块让位。
- 删除 `egui_titlebar_latch`（状态、setter、mouse 钩子、单测）；
  `has_iced_popup` 只剩「原生标题栏菜单 + 尚未迁移的工具栏下拉」。
- 验收：
  - `chrome_bands` 4/4：新增 `menu_opens_and_dispatches_actions`
    （点标签展开 → 点禁用项无动作 → 悬停子菜单 → 子项回传序号 2 并关闭）；
  - `egui_chrome` 单测 17/17（`titlebar_menu_model_resolves_every_action`
    覆盖 4 个菜单共 25 个动作；`iced_popups_disable_the_overlay` 改为
    「自绘标题栏菜单不再让位、原生标题栏仍让位」）；
  - 真实窗口（`LUMINO_EGUI_OVERLAY=1`）：点「文件」展开 14 项 + 2 分隔线，
    「导出为素材」灰显；悬停「导出工程」右侧展开两项子菜单且无重叠；
    点「设置」后 `Settings` 窗口出现（`830,432 900×675`），证明点击 → 序号 →
    `Message` → `process_message` 全链路通；
  - `clippy -D warnings`（ui / ui-egui / ui-parity）、`check --bin lumino-rs`、
    `capture_iced` 18/18、`capture_egui` 12/12、`compare_egui` 与上一步逐项一致
    （main_chrome 1.23%/1.87%/1.62%，settings 2.97%/3.55%/3.04%），基线文件零改动。

**Task 1.3（第九步）：精度 / 洋葱皮下拉迁移 — 已完成**

- 新增 `chrome/toolbar_popup.rs`：精度下拉（15 个预设 + `Custom…`）与洋葱皮
  范围菜单（4 项，矢量勾选）由 egui 画在 band 之外，几何对齐 iced
  （宽 190 / 内边距 6 / 项高 24 / 13px 文本 / 白色 10% 悬停）。
- 精度框与洋葱皮图标/下三角在 egui 工具栏里变为真实命中区，点击回传
  `ToolbarAction`（开合 / 预设 `SnapSpec` / 自定义 / 模式下标），生产侧
  `Root::toolbar_action_message` 翻译成与 iced 按钮同一批 `toolbar::Event`
  构造器；`Host` 在叠加层绘制后取走动作并 `process_message`。
- 占用矩形：两个弹层的触发区与面板都追加进 `menu::extend_blocked`，
  命中时 `handle_events` 提前返回——点击不再落到下层 iced。
- iced 侧 `status.rs` / `onion_skin.rs` 的 `DropDown` 在叠加层启用时强制关闭，
  只保留触发按钮占位，避免卷帘区出现第二份菜单。
- `has_iced_popup` 去掉 `snap_picker_open` / `onion_skin_menu_open`；
  只剩「原生标题栏菜单 + 悬浮工具条及其锚定面板 + 溢出菜单」。
- 验收：
  - `chrome_bands` 5/5：新增 `toolbar_popups_dispatch_snap_and_onion_actions`
    （点精度框 → `ToggleSnapPicker`；点预设 → `SnapSpec(1/4)`；点 Custom…
    → `SnapCustom`；点下三角 → `ToggleOnionMenu`；点第二项 → `OnionMode(1)`）；
  - `egui_chrome` 18/18：新增模型/动作测试（15 个预设且恰有一个当前项、
    4 个范围且默认选中「全部」、`SnapSpecChanged`/`OnionSkinModeSelected`
    解析、越界下标返回 `None`）；让位测试改为「精度/洋葱皮不再让位、
    悬浮工具条/溢出菜单仍让位」；
  - 真实窗口（`LUMINO_EGUI_OVERLAY=1`，ppp=1.25）：
    - 点精度框（屏幕 1694,94）→ 下拉完整展开 15 项 + `Custom…`，
      每项矢量音符小图分明；点「1/8」后弹层关闭、精度框文本变为 `1/8`；
    - 点洋葱皮下三角（2375,94）→ 右对齐展开 4 项且勾选「显示全部音轨」；
      点「显示上一个音轨」→ 勾选迁移到该项；日志确认动作序列为
      `ToggleOnionMenu` → `OnionMode(1)` → `ToggleOnionMenu`，无重复/泄漏；
    - 证据图：`target/snap-open-crop.png`、`target/snap-after2-crop.png`、
      `target/onion-final-crop.png`。
  - `clippy -D warnings`、`check --bin lumino-rs`、`capture_iced` 18/18、
    `capture_egui` 12/12、`compare_egui` 逐项与上一步一致，基线零改动。

**Task 1.3（第十步）：溢出菜单 + 折叠感知工具栏 — 已完成**

- 新增 `chrome/overflow.rs`：把生产 `compute_overflow` + `overflow_menu_items`
  镜像成图标网格（列优先、40px 按钮 / 20px 图标 / 4px 间距 / 8px 内边距，
  白 12% 悬停、`background.weakest×0.9` 面板底），右对齐窗口右缘内缩 4、
  顶边贴工具栏下沿；点击回传 `ToolbarAction::OverflowItem(index)`。
- **折叠结果接进 egui 工具栏**：`ToolbarModel` 新增 `groups`（8 个分组可见性）、
  `visible_tools`（工具前缀）、`collaboration_visible`、`more_visible`、
  `overflow_menu`。窄窗口下 egui 现在真的会隐藏被折叠的分组/工具，而不是
  继续画在右侧按钮下面。
- **修复协作按钮被假「更多」盖住**：无溢出时 iced 不画 More、协作图标常显；
  旧 egui 无条件画 More（不可用且遮住下层协作按钮）。现在 `more_visible` 来自
  真实折叠结果：无溢出画协作（点击穿透到 iced），有溢出画 More 并把协作收进
  菜单。
- **修复双击开关**：「更多」触发区此前只在菜单打开时进入占用列表，首次点击
  同时落到 iced，`ToggleOverflowMenu` 触发两次（开→关，面板闪一帧）。现在
  触发区**始终**占用；溢出项另按同一视口宽度重算解析
  （`Root::overflow_action_message(index, viewport_width)`）。
- iced 侧 `with_toolbar_overlay` 在叠加层启用时直接返回内容，不再画第二份；
  `has_iced_popup` 去掉 `overflow_menu_open`，只剩原生标题栏菜单 + 悬浮工具条
  及其锚定面板。
- 验收：
  - `chrome_bands` 5/5 + `overflow_bands` 2/2（网格项下标、禁用项无动作、
    点 More 开合、无溢出无触发矩形）；
  - `egui_chrome` 19/19（新增「宽窗口无 More/协作可见/工具全显」、
    「900px 折叠 + 溢出项逐个可解析 + 禁用项为 None + 越界 None」）；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 逐项与上一步一致，基线零改动；
  - 真实窗口：2048 逻辑宽（无溢出）右端显示协作图标（点击打开协作窗口）；
    1200×800 逻辑（溢出）显示 More → 12 项 4×3 网格；点第 12 项后日志
    `ToggleOverflowMenu` → `OverflowItem(11)` → 协作窗口出现；
  - 证据图：`target/wide-right.png`、`target/overflow-grid.png`、
    `target/overflow-click.png`。

**Task 1.3（第十一步）：悬浮工具条 + 五个锚定面板 — 已完成**

- 新增 `chrome/tool_panel.rs`（胶囊 + 拖拽 + 锚定定位）与
  `tool_panel/panels.rs`（画刷 / 形状 / 粗细）、`panels/fill.rs`（分音符填充）、
  `text_panel.rs` + `text_panel/font.rs`（文字参数 + 字体下拉）：
  - 胶囊几何完全对齐生产：34px 按钮 / 22px 图标 / 4px 间距 / 5px 内边距 /
    全圆角，左端拖拽柄（两根竖条）+ 两条分隔线 + 六个绘制工具 + 右端
    「粗细」与齿轮；
  - 面板宽 248、圆角 8、`background.weakest×0.9` 底 + `background.strong` 边框，
    锚定在**触发按钮正上方**（水平居中、越界吸附、顶部空间不足下翻）；
  - 分音符填充含真实 `TextEdit`（键盘输入经叠加层转发并只在文本编辑聚焦时
    消费，避免触发 iced 快捷键）；字体列表为自绘弹层，矩形同样进入占用列表。
- **折叠/拖拽语义复用生产状态机**：点击回传 `ToolPanelItem { kind, settings }`
  （`settings` 由生产按「已启用且有设置」算好），拖拽回传
  `ToolPanelDragStarted/Dragged/Ended`，生产侧继续用 `tool_panel_offset`
  累积位移并在松手时做默认位吸附。
- **修复拖拽收尾丢失**：释放帧 `grip.dragged()` 已为 false、只有
  `drag_stopped()` 为 true；旧写法会回退到未拖拽的胶囊响应丢掉结束事件，
  已把 `drag_stopped` 纳入响应选择并在释放帧优先回传 `DragEnded`。
- **修复输入框点击**：`TextEdit` 放在子 UI 中，命中区与生产输入框一致；
  点击后 `text_edit_focused=true`，键盘事件只给 egui。
- iced 侧 `view_draw_toolbar` 在叠加层启用时不再绘制（避免两份胶囊）；
  `has_iced_popup` 只剩「系统原生标题栏菜单」一项。
- 验收：
  - `tool_panel_bands` 5/5：条目/粗细/齿轮路由、形状选择与抽屉、粗细面板
    外部点击关闭、输入框键入回传、拖拽 Started→Ended（释放帧必须为 Ended）；
  - `egui_chrome` 21/21：新增悬浮条模型镜像（六条目、选中态、形状图标、
    五个面板内容）与动作翻译（含拖拽、关闭、文字参数、模式切换）；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 逐项与上一步一致，基线零改动；
  - 真实窗口（`LUMINO_EGUI_OVERLAY=1`，ppp=1.25）：
    - 「绘制工具箱」按钮 → 胶囊出现在卷帘底部居中（六工具 + 分隔 + 粗细 + 齿轮）；
    - 画刷两次点击 → 胶囊上方弹出「粗细设置 1 层 / 100」面板；
    - 点「粗细」→ 面板展开，拖滑杆 → `ThicknessChanged(70)` 且值文本更新；
    - 形状两次点击 → 三图形 + 齿轮面板，点圆形 → 胶囊形状图标变为圆形；
    - 文字两次点击 → 11 行完整参数面板（字体/字号/字重/斜体/单位/字距/行距/
      对齐/阈值/增长/模式），字体下拉展开且盖住面板内容；
    - 颜料桶两次点击 → 分音符填充面板，点击输入框后键入 `16` →
      `FillDivisionChanged("1")`、`FillDivisionChanged("16")`，输入框显示 `16`；
    - 拖拽柄拖动 120px → `ToolPanelDragStarted` → 多次 `ToolPanelDragged` →
      `ToolPanelDragEnded`，胶囊实际右移并保持新位置；
  - 证据图：`target/tp-open3.png`、`target/tp-brush2.png`、
    `target/tp-thickness3.png`、`target/tp-shape2.png`、`target/tp-text4.png`、
    `target/tp-font2.png`、`target/tp-typed.png`、`target/drag-after2.png`。

**Task 1.3（第十二步）：设置交互通道 + 快捷键/洋葱皮/兼容性页 — 已完成**

- **补上设置动作通道**：此前设置「常规」页只是可视化，交互从未回传生产（点下拉、
  改输入框、确认都不会生效）。新增 `settings/action.rs`：
  `SettingsAction`（菜单切换 / 橡皮擦 / 加轨 / 日志保留 / 洋葱皮后端 / GPU 三项 /
  检查 / 复制 / 确认）+ `take_action`；设置窗口的 Host 在绘制后取走动作，
  `Root::settings_action_message` 翻译成 `Message::Settings(...)` /
  `Message::SettingsDialog(CloseDialog)` 再走 `process_message`。
- **菜单真正可切页**：点击左侧菜单回传 `MenuSelected(index)`，生产
  `selected_menu_index` 更新；已迁移页面继续整窗接管，未迁移页面（音频等）自动
  交回 iced。
- **新迁移三页**（`settings/pages.rs`）：
  - 快捷键页：标题 + 占位说明（与 iced 同文案）；
  - 洋葱皮页：渲染后端 GPU/CPU 下拉（加载 MIDI 后只读展示 + 锁定提示）、
    CPU 能力边界提示；选择回传 `OnionSkinBackend`（生产锁定态会忽略切换）；
  - 兼容性页：检查 / 复制诊断按钮（检测中/冷却/锁定/无结果时禁用）、结果区
    按卡片宽度**自动换行**（修复真实窗口里多行详情互相重叠）、启动检查 /
    启动警告 / Domino 三个开关（非 Windows 下 Domino 只读置灰）。
- **General 页接线**：橡皮擦 / 加轨下拉与日志输入改为回传动作；确认按钮回传
  `Confirm` → 关闭设置窗口并应用。
- **模型**：`SettingsSnapshot.page: SettingsPageModel`（General / Shortcuts /
  OnionSkin / Compatibility）；页面文案与结果行全部由生产按当前语言预计算，
  `lumino-ui-egui` 不做 i18n。命中区（菜单行 / 下拉 / 按钮 / 开关）写入
  egui 内存，供无头测试与真实窗口自动化定位。
- 验收：
  - `settings_pages_bands` 4/4：菜单切页 / 确认 / 洋葱皮下拉 → CPU /
    检查按钮 + 两个开关；
  - `egui_chrome` 23/23：新增「仅已迁移页面接管（1 页交回 iced、3/4/8 接管，
    锁定态与开关镜像）」「设置动作全部翻译成生产 Message」；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 逐项与上一步一致，基线零改动；
  - 真实窗口（`LUMINO_EGUI_OVERLAY=1`，ppp=1.25，设置窗口 720×540 逻辑）：
    - 点「快捷键」→ 页面切换（日志 `MenuSelected(3)`，标题 + 占位说明）；
    - 点「洋葱皮」→ 下拉选 CPU → `OnionSkinBackend(Cpu)`，下拉与生产状态同步；
    - 点「兼容性」→「检查 GPU 兼容性」→ `RunGpuCheck`，结果换行显示且
      不覆盖按钮/开关；两个开关切换各回传一次动作，勾选状态与生产一致；
    - 常规页日志输入改为 25 → `LogRetention("2")` / `("25")`；
    - 点「确认」→ `Confirm` → 设置窗口关闭。
  - 证据图：`target/st-shortcuts.png`、`st-onion.png`、`st-onion-cpu.png`、
    `st-compat2.png`、`st-compat3.png`、`st-compat-toggle2.png`、`st-log.png`。

**Task 1.3（第十三步）：音频页（XSynth / LGS / 系统 MIDI）— 已完成**

- 新增 `settings_audio.rs`（页面模型构建 + XSynth 行）、
  `settings_audio/pages.rs`（LGS / 系统 MIDI 行）、
  `settings_audio/actions.rs`（字段动作 → 生产 `settings::Event`）：
  - 顶层输出类型（内置合成器 / KDMAPI / 系统 MIDI）与内置引擎（XSynth / LGS）
    下拉切换，页面随后端动态变化；
  - 音频播放输出设备 / WinMM 播表 / MIDI 输入设备下拉 + 刷新按钮（无设备时灰字）；
  - XSynth：音色库路径 + 浏览、缓冲（5–100ms）、每键最大同音数（滑杆 + 自定义
    输入）、全局复音上限、复音软目标比例、过载保命闸、力度过滤；
  - LGS：音色库、块大小（2^6–2^13）、离线导出块长、每键最大同音数、全局复音
    上限、保命闸、响度过滤、GPU 缓冲状态 / 治理指标开关、LOAD 显示口径；
  - 系统 MIDI：WinMM 输出设备（播报表）。
- 页面内容可能高于卡片：新增页面级 `ScrollArea`（常规页保持原几何，像素基线
  不变）；兼容性页过长的结果也不再被裁掉。
- 修复真实窗口两处缺陷：
  1. 切换输出类型后，新页面同位置的滑杆被上一次弹层点击的自动 ID 误命中并
     回传 `AudioSlider`；给每行滑杆/输入框加稳定 ID，并把比例值对齐到步进网格
     （非网格初值会被 egui 首帧纠正并误报 changed）。
  2. 右侧控件行的长标签（如「全局最大复音数: 自动 (16384)」）会压到滑杆上；
     改为按可用宽度截断并加省略号。
- `SettingsAction::push_action` 改为**一帧只保留第一个动作**，避免页面重排时新
  控件被同一帧的点击连带触发。
- 验收：
  - `settings_pages_bands` 5/5：新增音频页字段动作（下拉 / 开关 / 按钮）；
  - `egui_chrome` 24/24：新增音频页三种后端行镜像 + 全部字段动作翻译；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 逐项与上一步一致，基线零改动；
  - 真实窗口（720×540 逻辑）：切到音频页（`MenuSelected(1)`）→ KDMAPI 页只有
    输出类型与 MIDI 设备；切「内置合成器」→ XSynth 行（`AudioCombo(OutputType,0)`）；
    引擎切 LGS（`AudioCombo(Engine,1)`）→ LGS 行、滚动正常；切换保命闸
    （`AudioToggle(LgsGate,false)`）；确认关闭（`Confirm`），下次打开仍为 LGS；
    长标签截断后不再压住滑杆。
  - 证据图：`target/au-xsynth2.png`、`au-lgs.png`、`au-lgs2.png`、
    `au-lgs-toggle.png`、`au-final.png`。

**Task 1.3（第十四步）：界面页（主题卡片 / 字体 / 交互 / 自动滚动）— 已完成**

- 新增 `model/settings_ui.rs`：`UiPageModel { title, rows }`，行类型覆盖
  Section / Combo / Text / Checkbox / Slider / Button / Static / ThemeCards；
  `UiField` 标识每个可交互项；主题卡片携带由生产按主题调色板预计算的全部
  预览色（背景 / 标题 / 迷你面板 / 三级文字与分隔线）。
- 新增渲染 `settings/ui.rs` + `ui/widgets.rs`：
  - 语言偏好下拉（跟随系统 + 全部具体语言，切页即时生效）；
  - 主题卡片网格：158×96 固定比例、按卡片区宽度自适应列数，选中态用当前强调色
    2px 描边、悬停加亮；点击回传规范主题名 → `window::Event::Theme`；
  - 字体：系统字体下拉 + 自定义路径输入 + 浏览按钮 + 重启提示；
  - 原生标题栏（macOS 隐藏）/ HiDPI 图标 / 音符总量显示位置；
  - 自动滚动三行（固定位置 / 翻页触发 / 翻页回到），后缀「像素（从…算起）」
    预留宽度不再溢出卡片；
  - 交互小节（框选框模式 / 256 键 / 力度曲线 / 播放键盘颜色）与两个滑杆
    （自动化线粗细 / 监控刷新间隔，右侧数值）。
- 页面纳入统一 `ScrollArea`；主题卡片多（高对比度 + `Theme::ALL`）时可滚动。
- 修复真实窗口布局问题：长标签按可用宽度截断（不再压住滑杆）、滑杆宽度预算
  调整为 180+48 给标签留空间、文本行按后缀实际宽度左移输入框。
- 验收：
  - `settings_pages_bands` 6/6：新增界面页主题卡片 / 开关 / 语言下拉；
  - `egui_chrome` 25/25：新增界面页行镜像（含主题卡片数量与高对比度首卡）与
    全部字段动作翻译；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 逐项与上一步一致，基线零改动；
  - 真实窗口（720×540 逻辑）：切到界面页（`MenuSelected(2)`）→ 语言下拉、
    主题卡片网格；点 Dracula 卡片 → `UiTheme("Dracula")`，整窗立刻换主题且卡片
    选中描边跟随；滚动到字体区（字体下拉 / 路径 / 浏览 / 重启提示）、
    原生标题栏与 HiDPI 开关；滚到底部切换 256 键
    （`UiToggle { Enable256Key, true }`）、确认关闭；自动滚动后缀完整显示。
  - 证据图：`target/ui-page.png`、`ui-dracula.png`、`ui-scrolled.png`、
    `ui-bottom3.png`、`ui-bottom4.png`、`ui-toggle.png`。

**Task 1.3（第十五步）：编辑 / 调色板 / 关于页 — 已完成**

- 复用界面页的通用行模型（`UiPageModel` / `UiRow` / `UiField`），新增行：
  `Swatches`（28×28 色块、每行 12 个、圆角预览容器）、`AboutLogo`
  （64px logo，摇晃偏移由生产状态给出，可点击）、`EchoCave`
  （打字机前缀 + 闪烁不透明度，整行可点）。
- 三页内容：
  - **编辑**：操作历史（总条数 / 单条上限 / 合并窗口）、自动保存（开关 + 间隔）、
    Tempo BPM 上限（预设 256…65536 + 自定义入口，选自定义打开输入弹窗）、
    编辑拦截 Toast 开关；
  - **调色板**：调色板下拉（加载 MIDI 后锁定为只读提示）、颜色预览标题、
    当前调色板全部色块网格；
  - **关于**：logo（点击上报彩蛋）、标题 / 应用名 / 版本 / 描述、回声洞文本
    （点击切换下一条）。
- 动作沿用 `UiText/UiToggle/UiCombo/UiButton`，字段扩展 10 个；`settings_ui/actions`
  统一翻译成 `settings::Event`（含 Tempo 自定义入口与调色板规范名）。
- 验收：
  - `settings_pages_bands` 6/6 + `settings_extra_bands` 1/1（文本 / 开关 /
    logo / 回声洞点击）；
  - `egui_chrome` 25/25 + `settings_audio_tests` 1/1（三页行镜像与动作翻译）；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 与上一步逐项一致，基线零改动；
  - 真实窗口（720×540 逻辑）：调色板页色块网格与下拉；编辑页文本/开关
    （`UiToggle { AutosaveEnabled }`）；关于页 logo 与回声洞点击
    （`UiButton { AboutLogoClicked / EchoCaveClicked }`）；确认关闭。
  - 证据图：`target/rem-palette.png`、`rem-editing2.png`、
    `rem-editing-toggle.png`、`rem-about.png`、`rem-echo.png`。

**Task 1.3（第十六步）：云管理页 + 设置窗口输入独占 — 已完成**

- 新增 `CloudPageModel / CloudConnModel`（连接 id / 名称+协议 / 地址 / 状态 /
  在线标记 / 三个按钮文本）与 `settings/cloud.rs`：告警条按实际宽度换行、
  「添加连接」强调按钮、连接列表（左侧名称+地址，右侧状态圆点+断开/管理/删除）；
  在线状态用**自绘绿/红圆点**替代 emoji（egui 默认字体缺 🟢/🔴 字形）。
- 动作：`CloudAdd / CloudConnect(id) / CloudDisconnect(id) / CloudManage /
  CloudDelete(id)` → `Message::Cloud(CloudAction::...)`；离线连接的「管理文件」
  禁用（无动作）。
- **设置窗口整窗消费输入**：10/10 设置页已由 egui 绘制，设置对话框的鼠标按键
  只给 egui，不再落到下层 iced。修复真实缺陷：云管理「添加连接」原先会同时
  触发 egui 与 iced，打开**两个**「添加云存储」窗口。
- **标题栏按钮接通**：自绘最小化 / 最大化 / 关闭回传
  `WindowTraffic(Minimize/ToggleMaximize/Close)`，关闭映射为
  `SettingsDialogAction::CloseDialog`，最小化/最大化映射为
  `window::Event::traffic_action`。保存确认等其它对话框暂不消费输入
  （其按钮仍依赖让位）。
- 验收：
  - `settings_cloud_bands` 1/1（添加 / 断开 / 连接 / 删除 / 管理 / 离线禁用）；
  - `settings_cloud_tests` 1/1（连接行镜像 + 5 个 CloudAction 翻译）；
  - `egui_chrome` 27/27；`ui-egui` 全量、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；`capture_iced` 18/18、
    `capture_egui` 12/12、`compare_egui` 与上一步逐项一致，基线零改动；
  - 真实窗口：云管理页显示两条真实连接（CBMS [SFTP] / 111 [WebDAV]）与
    绿点「在线」；点「管理文件」→ `CloudManage`；点「添加连接」→ `CloudAdd`
    且**只出现一个**「添加云存储」窗口；点标题栏关闭 →
    `WindowTraffic(Close)` 且设置窗口关闭。
  - 证据图：`target/cl-page2.png`、`cl-final.png`、`cl-add2.png`。

**Task 1.3（第十七步）：右侧栏「图片转 MIDI」面板 + 弹层重绘修复 — 已完成**

- 新增 `chrome/model/right_panel.rs`（`RightSidebarPanelModel` / `I2mPanelModel`
  / 行与控件枚举 / `I2mField`）与 `chrome/right_panel.rs`（渲染 + 动作通道），
  并把面板 band 扩到「`panel_width` + 48 图标列」：左侧 6px 拖拽手柄继续交给
  iced（缩放拖拽照常），内容区 `panel_width - 6` 整块进入 egui 占用列表。
- 面板内容对齐生产：标题 / 「选择图片文件」/ 「转换参数」/ Key 范围（双输入 +
  「~」）/ 目标高度 / 每像素 tick / 颜色数 / 调色板下拉；按钮与输入框
  （42×22、`background.weak` 底、1px `background.strong` 边框）逐项复刻；
  「转换为 MIDI」仅在已选择文件时出现，转换中禁用并改文案。
- 动作：`SelectImageFile / I2mConfigTextChanged(field,text) / I2mPaletteChanged /
  ConvertClicked` → `Message::RightSidebar(...)`；生产侧在
  `egui_chrome/right_sidebar.rs` 统一构建模型与翻译动作。
- 叠加层启用时 iced 的 I2M 面板内容（`view.rs` 分发）改为同宽占位，避免两份
  文字 / 输入框重影；素材库与瀑布流仍走 iced（拖拽出卷帘 / gfx 纹理依赖）。
- **修复两个真实缺陷**：
  1. 编辑键（退格等）在部分平台同时带控制字符 `text`，被当作文本插入输入框，
     与 `Key::Backspace` 叠加污染缓冲；`OverlayInput` 增加可打印字符过滤
     （含单测）。
  2. egui 弹层（ComboBox / Tooltip）首帧是「测量 pass」不产生图元，叠加层没有
     按 `has_requested_repaint()` 再排一帧，导致主窗口下拉菜单要等下一次鼠标
     事件才出现；现在按需补一帧重绘。
- 验收：
  - `right_sidebar_bands` 5/5：band 几何（194 宽 / 右缘贴图标列 / 手柄不占用）、
    选择图片、配置输入（键入回传）、调色板下拉（选中下标）、转换按钮可见性；
  - `egui_chrome` 29/29：新增 I2M 模型镜像（5 行、调色板下标、已选文件提示、
    转换中禁用）与四个动作翻译；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 与上一步逐项一致，基线零改动；
  - 真实窗口（ppp=1.25）：点右栏 I2M 图标 → 面板按生产几何出现；
    「选择图片文件」→ `SelectImageFile` 且只弹出一个文件对话框；
    目标高度输入框退格清空再键入 → `I2mConfigTextChanged` 依次
    `""` / `"4"` / `"42"`，输入框显示 42；
    调色板下拉正常展开（17 个本地化选项）→ 选「八叉树」→
    `I2mPaletteChanged(3)` 且显示同步；拖拽左缘手柄 → 面板变宽、控件重新右对齐，
    iced 缩放链路未被占用列表打断。
  - 证据图：`target/rs-final-panel.png`、`rs-final-palette.png`、
    `rs-palette-picked2.png`、`rs-typed3.png`、`rs-resized.png`。

**Task 1.3（第十八步）：右侧栏素材库面板 + 拖出桥 — 已完成**

- 新增 `MaterialsPanelModel`（标题 / 添加按钮与下拉 / 分区列表 / 重命名缓冲 /
  右键菜单 + 文案 / 悬停提示行）与 `right_panel/materials.rs` +
  `materials/ctx.rs`：列表条目 `background.weaker` 底、圆角 4、内边距 `[6,8]`；
  添加按钮全宽主色（`primary.weak` → 悬停 `primary.base`）；右键菜单 56×144
  （40px 图标按钮 ×3、近黑底、圆角 8），在鼠标右下方展开并在贴近右/下缘时
  翻转；内置素材菜单项置灰。
- 交互全部接通：添加菜单开合 / 从 web 下载 / 从本地选取、素材右键菜单
  （重命名 / 删除 / 上传到云）、重命名输入与回车确认、覆盖背景关闭、
  悬停提示（名称 / 作者 / 位置 / 轨道数 / 来源）。
- **拖出桥**：素材项在**按下瞬间**回传 `MaterialDragStarted(index)`（不是
  click 释放），跟随预览与松手放置继续由 iced 编辑器状态机驱动；面板内容区
  整块占用，按下不会同时落到 iced，而光标移动与卷帘内释放仍走 iced。
- 右键位置：`MaterialContextMenuOpened { index, x, y }` 在生产侧展开为
  `Batch([CursorMoved(x,y), ContextMenuOpened(index)])`，保证菜单锚定在
  按下时的面板局部坐标。
- 叠加层启用时 iced 的素材库面板内容改为同宽占位，避免两份列表 / 菜单重影；
  删除确认弹窗仍由 iced 主窗口叠加层承接（不迁移，避免削减功能）。
- 验收：
  - `materials_bands` 6/6：添加菜单、有效素材按下拖出、无效素材无动作、
    右键坐标（面板局部）、菜单项（可编辑 / 置灰）、覆盖背景关闭、重命名
    输入与回车；
  - `egui_chrome` 31/31：素材库模型镜像（分区 / 重命名 / 右键目标与位置 /
    添加菜单 / 悬停行）与全部动作翻译（含 Batch 顺序）；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 与上一步逐项一致，基线零改动；
  - 真实窗口（ppp=1.25）：素材库显示 11 个内置素材（扫描 12 个含测试用户素材）；
    添加菜单展开两项，「从本地选取」只弹出一个文件对话框
    （`MaterialMenu(ImportFromLocal)`）；内置素材右键菜单置灰且点击无动作，
    点空白 → `MaterialContextMenuClosed`；用户素材右键菜单可用，
    重命名输入 `MaterialRenameInput` → 回车 `MaterialRenameConfirmed` 且磁盘
    文件已改名；按住「圆」拖入卷帘 → 日志「已进入拖出跟随模式」、拖拽中截图
    可见预览跟随，松手进入放置预览（Placing）。
  - 证据图：`target/mat-panel2.png`、`mat-menu.png`、`mat-ctx.png`、
    `mat-import.png`、`mat-user-ctx2.png`、`mat-rename.png`、
    `mat-renamed2.png`、`mat-drag-mid.png`、`mat-placing.png`。

**Task 1.3（第十九步）：保存确认对话框按钮与标题栏动作 — 已完成**

- `lumino-ui-egui/src/dialog.rs` 新增 `SaveConfirmAction`
  （`Save / Discard / Cancel / DragWindow / Minimize / ToggleMaximize /
  CloseWindow`）与独立动作通道；三个内容按钮（90×35、间距 10、水平居中）、
  自制标题栏红绿灯、标题栏空白区按下拖动全部接通。`DialogPalette` 补
  `primary_hover`（`primary.strong`）与 `secondary_hover`
  （`background.strong`），悬停色与 iced 一致。
- 生产侧 `save_confirm_action_message` 映射：`Save / Discard / Cancel` →
  `Message::SaveConfirm(...)`；窗口关闭按钮等价于取消；`DragWindow` →
  `window::Event::drag()`；最小化 / 最大化 → `TrafficAction`。
- 输入独占：`dialog_input_exclusive()` 覆盖 `Settings | SaveConfirm`，
  保存确认窗口的鼠标按键只给 egui，不再落到同窗口的 iced 按钮
  （否则同一击会触发两次动作）。
- **修复真实错位/裁切缺陷**：对话框窗口按内容测量为 169 逻辑高（内容区
  139），原先固定的 `+126` 按钮顶边会越过窗口底边把三个按钮裁掉。现在
  保持场景基线偏移不变，仅在会溢出时整体上移（底部保留 24px），
  场景像素基线与 `compare_egui` 数字零变化。
- 验收：
  - `save_confirm_bands` 5/5：三个内容按钮、标题栏红绿灯、
    标题栏拖动（且红绿灯不在拖动区）、原生标题栏模式无标题栏命中区；
  - `egui_chrome` 32/32：七个动作到生产消息的映射（含关闭按钮 = Cancel）；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 逐项与上一步一致，基线零改动；
  - 真实窗口（ppp=1.25）：改 BPM 置脏后关闭主窗口，保存确认窗口
    424×211 物理（338.75×169 逻辑），提示与三个按钮完整可见；
    点「取消」→ 日志 `egui 保存确认动作：Cancel`、对话框关闭、主窗口保留；
    点「关闭」（Discard）→ 日志 `Discard` 且应用退出。
  - 证据图：`target/sc-dlg1.png`、`sc-dlg2.png`、`sc-w4.png`、`sc-w5.png`。
- 环境注意：反复 `Stop-Process` 会留下
  `config/autosave/autosave.lmpj`，下次启动的原生「恢复自动保存」模态框
  在本机会以隐藏窗口阻塞 UI 线程（表现为界面不响应）；删除该文件后恢复正常。
  这是既有启动提示行为，与本步改动无关。

**Task 1.3（第二十步）：系统原生标题栏菜单由 egui 接管 — 已完成**

- `TitlebarModel` 新增 `menu_only`：系统原生标题栏模式下，egui 只画客户端顶部
  30px 菜单栏（从 x=0 起、无 logo / 窗口控制 / 拖动区），标签宽 = 文本宽 + 17
  （左右 8px 内边距 + 1px 间距），与 iced `menu::view_placeholder` 同几何。
- iced 侧 `view_native_titlebar` 在叠加层启用时改用
  `menu::view_placeholder`（同宽占位、不吃输入、不画下拉），原生模式的菜单
  标签与下拉全部由 egui 绘制。
- **移除 `has_iced_popup` 让位**：`egui_chrome_model_for` 不再在
  `use_native_titlebar && is_menu_open` 时返回空模型；原生模式与自绘标题栏
  共用同一套菜单状态机、下拉面板与动作通道。
- 顺带修正：原生模式（Windows/Linux）顶部仍有 30px 菜单栏行，
  `PianoRollView.top_inset` 与工具栏 / 右侧栏 / 悬浮工具条的顶部偏移统一按
  「标题栏行 + 工具栏」计算（macOS 原生模式没有该行，保持 0）。
- **修复悬停切换后的点击缺陷**：菜单展开时悬停到其他标签会切换展开项，
  随后点击该标签会被误判为「再次点击关闭」。`MenuState` 增加
  `hover_opened`：悬停切换后点击保持展开，点击其他标签或菜单项 / 空白 /
  Esc 时复位。
- 验收：
  - `menubar_bands` 4/4：菜单从 x=0 起、无 logo / 无窗口控制、标签首尾相接；
    点击标签展开面板并回传动作序号；悬停切换后点击保持展开；
  - `egui_chrome` 32/32：原生标题栏 + `is_menu_open` 时仍返回菜单栏 /
    工具栏 / 右侧栏模型（不再整块让位），菜单标签与自绘模式一致；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 逐项与上一步一致，基线零改动；
  - 真实窗口（ppp=1.25，临时将配置切到原生标题栏 + Tokyo Night Storm 验证后
    已还原）：系统标题栏下方出现 egui 菜单栏（文件/编辑/视图/帮助，从 x=0 起）；
    点「文件」展开下拉，再点「工程设置」打开工程设置窗口；
    先开「编辑」（误点为「视图」）再悬停/点击「文件」，菜单保持展开不关闭。
  - 证据图：`target/native-2.png`、`native-top2.png`、`native-filemenu2.png`、
    `native-switch1.png`、`native-switch2.png`、`native-settings.png`。

**Task 1.3（第二十一步）：egui 外壳跟随 iced 主题 — 已完成**

- `lumino-ui-egui/src/theme.rs` 从「Tokyo Night Storm 常量」改为语义色板
  `Palette` + `PaletteSeed`：所有 `theme::XXX` 常量改为按当前色板取色的
  访问函数（182 处调用点机械替换，渲染代码不改结构）。
- 生产侧新增 `egui_chrome/palette.rs`：`Root::egui_palette()` 直接从
  `Theme::extended_palette()` 取 `background.* / primary.* / success /
  warning / danger`，再由 `Palette::from_seed` 派生 iced 没有直接给出的
  颜色（边框 / 次级文字 / 悬停填充 / 浅色卡片底色 / 强调色上的文字）。
  叠加层每帧在 `run_ui` 前 `set_palette`，因此 Light / Dark / Solarized /
  Catppuccin / Tokyo Night 等所有内置主题都与 iced 同色。
- **修复默认配置（Light）下的双主题叠层**：此前 egui 外壳固定深色，而
  iced 内容是浅色；现在标题栏 / 工具栏 / 左栏 / 状态栏 / 对话框全部跟随。
- 顺带修复的主题相关缺陷：
  - `Icon::default_tint` 原为固定白色 → 浅色主题下所有图标不可见；改为
    深色染白、浅色染黑（与 iced `should_invert_icon` 同规则）；
  - 浅色主题滚动条滑块原为正文色（纯黑），改为 iced 的
    `background.strong`；深色主题保持历史口径；
  - 强调色按钮文字统一走 `theme::on_accent()`（浅色主题为白字，深色为
    深字），替换 4 处硬编码 `#121A2A`；
  - 设置 / 保存确认对话框的调色板改由 `Root::egui_palette()` 派生，
    不再有「深色常量兜底」分支。
- 验收：
  - `theme_palette` 4/4：TNS 种子逐色还原历史字面量、Light 种子派生浅色
    外壳（含次级文字对比度）、深色种子仍为深色、**实际渲染**的 band
    底色随色板变化（浅色 #D6D6D6 / #DEDEDE，深色 #3F466E）；
  - `egui_chrome` 33/33（新增 `egui_palette_follows_the_window_theme`：
    Light 配置派生浅色、TNS 精确还原、高对比度主题仍为黑底）；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 逐项与上一步完全一致，`tests/ui-baseline` 零改动；
  - 真实窗口（ppp=1.25，用户默认 Light 配置）：标题栏 / 工具栏 / 左侧栏 /
    状态栏与 iced 内容同色，图标深色可见、状态栏次级文字可读；
    文件菜单、设置对话框（含滚动条）、工程设置、保存确认对话框均为浅色，
    主按钮白字；取消 → `Cancel` + 对话框关闭、关闭（Discard）→ 应用退出。
  - 证据图：`target/theme-after2.png`、`theme-before.png`、`iced-light.png`、
    `light4-settings.png`、`light5-savedlg.png`、`light3-filemenu-v.png`。

**Task 1.3（第二十二步）：钢琴瀑布流面板接入 egui（gfx 纹理通道拆分）— 已完成**

- **桥接层外部纹理**：`lumino-egui-bridge` 的 `Painter` 新增
  `set_external_texture / clear_external_texture`，按 `Arc<TextureView>` 指针
  识别同一张纹理（重复注册空操作、离屏纹理重建自动换绑）；egui 图元采样时
  先查自身上传纹理，再查外部纹理。`painter.rs` 拆成
  `painter.rs` / `painter/texture.rs` / `painter/scissor.rs`（< 400 行）。
- **模型**：`RightSidebarPanelModel::PianoWaterfall(WaterfallPanelModel)`
  （标题 / 占位文案 / `Option<egui::TextureId>`）+
  `WATERFALL_TEXTURE_ID` 常量；渲染在 `right_panel/waterfall.rs`：
  标题 14px、间距 8、内容区 1:1 铺满（与 iced 同几何），纹理未就绪时
  居中显示 11px 占位文案。
- **生产接线**：`egui_chrome/right_sidebar.rs` 为 PianoWaterfall 返回内容
  模型（纹理就绪时带 ID）；`render_egui_overlay` 每帧把
  `right_sidebar.piano_waterfall.waterfall_view`（`lumino-gfx` 离屏渲染产物）
  注册到桥接层，面板关闭即注销。
- **消除二次采样**：叠加层启用时 iced `piano_waterfall::panel` 只返回同宽同高
  占位（保留布局、不建 shader 图元），同一张离屏纹理只被 egui 采样一次。
- 验收：
  - `egui-bridge` 6/6：新增「外部纹理颜色往返」「视图换绑后采样新纹理」，
    真实 GPU + 回读，色差 ≤1；
  - `right_sidebar_bands` 7/7：瀑布流面板几何（内容宽 194、顶边 102）、
    内容区整块占用、6px 手柄留给 iced、带外部纹理 ID 时点击不产生动作；
  - `egui_chrome` 34/34：瀑布流面板返回内容模型（标题 / 占位文案 / 无纹理
    时不携带 ID），素材库与瀑布流均不再是 `None`；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 逐项与上一步一致，基线零改动；
  - 真实窗口（ppp=1.25、Light）：点右侧栏第三个图标打开「钢琴瀑布流预览」，
    标题与底部键盘可见；与关闭叠加层时的 iced 渲染逐像素对照，键盘条位置、
    黑键分布、面板宽度一致（仅字体/工具提示差异），无重影。
  - 证据图：`target/wf2-panel.png`、`wf2-keyboard.png`（egui）、
    `wf-panel-iced.png`、`wf-iced-keyboard.png`（iced 对照）。

**Task 1.3（第二十三步）：卷帘骨架接管开关打通（`LUMINO_EGUI_PIANOROLL`）— 已完成（默认仍关闭）**

- **egui 骨架配色改为生产口径**：`Palette` 新增 6 个卷帘色
  （`roll_keyboard_bg` / `roll_white_key` / `roll_black_key` / `roll_key_line`
  / `roll_ruler_bg` / `roll_text`），逐条对应 iced `ThemeExt`
  （`keyboard_background_color` / `white_key_color` / `black_key_color` /
  `key_line_color` / `ruler_background_color` / `text_color`）。
  `PianoRollView` 骨架不再用通用 `base/panel` 常量：亮色键盘 = `weak`、
  黑键 = `strong`、键线 = `strongest`、标尺 = `weakest`、文字 = 黑；
  256 键扩展区按 iced 规则（亮 ×0.85 / 暗 ×1.15）。
- **修复真实窗口暴露的两处错位**：
  - 键线由「每个键四周 1px 描边」改为「每键顶边一条 1 物理像素线」，
    与 gfx 键盘的单线边界一致（原先相邻键的双描边把每个键挤掉 2px）；
  - 键盘列左上角（标尺同高）按生产画成标尺底色 + 顶部键音名，
    不再把被压住的键填成黑键色（原先是一条 120×30 的深色块）。
- **gfx 通道让位**：`RenderParams` 新增 `skip_roll_chrome`；置位时网格绘制的
  裁剪区去掉键盘列与标尺行（`skip_roll_chrome_scissor` + 单测），
  `RulerRenderer` 不再提交，音符 / 洋葱皮 / 网格线 / CC 条全部保留。
- **接线**：`Root::egui_piano_roll_active()` 与骨架模型**同源**
  （overlay + gate + `egui_piano_roll_model().is_some()`），模型为空
  （循环区间 / 中途变拍号 / 洋葱皮 / 协作光标 / 非卷帘视图）时 gfx 仍画骨架；
  `params.rs` 用该值设置 `skip_roll_chrome`。
- 验收：
  - `theme_palette` 5/5：新增「亮色卷帘骨架实渲染」——键盘底 / 白键 / 黑键 /
    键线 / 标尺 / 文字六种颜色与生产实测值逐字节一致；
  - `lumino-gfx` 273/273（新增 3 例裁剪区单测）；`egui_chrome` 34/34；
  - `ui-egui` 全量、`ui-parity` 12/12；`clippy -D warnings`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12、
    `compare_egui` 逐项与上一步一致，`tests/ui-baseline` 零改动；
  - 真实窗口（ppp=1.25、Light、`LUMINO_EGUI_PIANOROLL=1`）：键盘行颜色
    222/206、边界 190、标尺 245、角上 G#9 与生产逐像素一致（剩余差异仅为
    字体抗锯齿）；网格区域与关闭骨架时逐像素一致（仅 939 px 网格线 AA 差异）；
    关闭 gate 后键盘/标尺仍由 gfx 绘制，行颜色与改动前一致。
  - 证据图：`target/roll-gate3.png`、`roll-gate-keys3.png`（gate 开）、
    `roll-off2.png`、`roll-off-keys-zoom.png`（gate 关 = gfx）。

**Task 1.3（第二十四步）：卷帘骨架默认接管（删除门控）+ 暗色生产口径 + 基线重算 — 已完成**

- **默认接管**：删除 `egui_chrome/gates.rs` 与 `LUMINO_EGUI_PIANOROLL`
  （`Root::egui_piano_roll_model()` 直接进入模型）；`egui_piano_roll_active()`
  只剩「overlay 开 + 模型非空」两个条件，模型为空时 gfx 继续画骨架。
- **暗色切生产口径**：骨架统一走 `roll_*` 色板（不再有「暗色 50% 混合描边」的
  历史分支）；并修正一处真实差异——生产 gfx 的暗色键盘**没有可见键线**
  （只靠键色区分），亮色才有 1 像素 `strongest` 键线；左上角（标尺同高）
  统一画标尺底色 + 顶部键音名，与 gfx 一致。
- **基线重算（有意变更）**：`main_chrome` / `piano_roll_skeleton` 各 3 张 egui
  基线按新骨架重算（`capture_egui` 12/12 校验通过）。`compare_egui` 对 iced
  基线的差异随之变化（piano_roll 1.00: 19514→26186；1.25: 53658→45789；
  main_chrome 1.00: 20212→26439；1.25: 55255→47955）——这是向**生产 gfx**
  对齐的结果，不是回归；真实窗口逐像素对照见下。
- 验收：
  - `bands_pixels` 通过（暗色首行断言改为白键底色 = 生产无键线）；
    `theme_palette` 5/5；`egui_chrome` 34/34；`lumino-gfx` 273/273；
    `ui-egui` 全量、`ui-parity` 12/12；
  - `clippy -D warnings`、`check --bin lumino-rs`；`capture_iced` 18/18、
    `capture_egui` 12/12（6 张新基线）；
  - 真实窗口（ppp=1.25）不设任何卷帘环境变量：
    - 暗色（临时切 Tokyo Night Storm，验证后已还原 Light）：
      键盘行 `36/42` 与 overlay 关闭（gfx）逐像素一致，网格区域 **0 像素差异**，
      标尺仅文字抗锯齿差；键盘标签行有字体抗锯齿差；
    - 亮色：键盘行 `245/190/222/206` 与 gfx 一致，网格区域仅 939 px 网格线 AA。
  - 证据图：`target/light-final.png`、`dark-on2.png`、`dark-on2-keys.png`、
    `dark-off.png`、`dark-off-keys.png`。

**Task 1.3（第二十五步）：工程设置对话框迁移到 egui — 已完成**

- **模型**：`ProjectSettingsModel`（标题 / 六个输入字段与占位 / 只读创建日期与
  累计创作时间 / 可选的音符总量行 / 按钮文案 / `titlebar` 标记），
  `DialogModel::ProjectSettings(Box<...>)`。
- **渲染**：`ui-egui/src/project_settings.rs` 整窗绘制内容：24px 内边距、标题
  18px、标签与只读值 14px、输入框 `background.base` 底 + 1px `strong` 边框、
  圆角 4、BPM 行带单位、拍号 60 宽双输入、底部取消 / 确定（100×34、
  间距 12），表单在 8px 实体滚动条内滚动（与生产 `scrollable(form)` 一致）。
- **关键修复（UI 元素不被削减）**：工程设置窗口是非原生标题栏模式下的
  **自制 iced 标题栏**（30px，含最小化/最大化/关闭与拖动）。egui 只接管
  标题栏以下的内容区：顶部 30px 继续由 iced 绘制，`dialog_input_exclusive`
  对 ProjectSettings 在该区域内不消费点击（红绿灯 / 拖动仍可用）。
- **动作通道**：`NameChanged / TempoChanged / CopyrightChanged / AuthorChanged /
  NumeratorChanged / DenominatorChanged / Confirm / Cancel` →
  `Message::ProjectSettings(...)`（取消 = `CloseDialog`），在
  `egui_overlay` 里统一处理；输入合法性仍由生产侧校验。
- 代码组织：`egui_chrome/dialogs.rs` 拆出 `dialogs/project_settings.rs`
  （模型装配 + 动作映射），两个文件分别 340 / 79 行。
- 验收：
  - `project_settings_bands` 3/3：确定 / 取消按钮、名称输入回传新文本、
    只读行与全部标签进入可访问树；
  - `egui_chrome` 36/36：模型镜像（字段 / 拍号 / 创建日期空值→「未知」/
    音符总量两位置互斥 / 标题栏标记）与八个动作映射；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；`capture_iced` 18/18、
    `capture_egui` 12/12，基线零改动；
  - 真实窗口（ppp=1.25、Light）：工程设置对话框顶部 iced 自制标题栏完整可见，
    内容区布局与 iced 版本一致（输入框白底细边框、BPM 单位、拍号 60 宽、
    按钮在底部）；BPM 改为 96 → 日志 `egui 工程设置动作：TempoChanged("96")`，
    点「确定」→ `Confirm` + `应用工程设置 ... BPM=96` 且对话框关闭；
    点标题栏「×」可正常关闭（iced 标题栏未被遮挡）。
  - 证据图：`target/ps3-screen.png`、`ps2-dialog.png`、`ps2-scroll2.png`、
    `ps2-edit.png`、`ps-off-screen.png`（iced 对照）。

**Task 1.3（第二十六步）：撤销历史对话框迁移到 egui — 已完成**

- **模型**：`HistoryModel`（`titlebar` / 标题 / 提示 / 行列表 / 计数 / 关闭文案）+
  `HistoryRowModel { index, label, current, future }`；
  `DialogModel::History(Box<...>)`。
- **渲染**：`ui-egui/src/history.rs` 对齐生产：整窗 base 底 + 1px `strong` 边框
  （圆角 8）、内边距 16、标题 18px、提示 12px、行高 32 + 间距 2；
  当前行 `primary.weak` 底 +「▶」标记、未来步骤 `strong.text` 浅色、悬停 6% 白；
  行文本左对齐（不是按钮居中）；底部「current / total」+ 主色「关闭」按钮；
  列表 8px 实体滚动条。标题 / 提示 / 计数用 Label 进入可访问树。
- **标题栏让位**：与工程设置一致——顶部 30px iced 自制标题栏（红绿灯 / 拖动）
  仍由 iced 绘制，`dialog_input_exclusive` 对 History 在该区域内不消费点击。
- **动作**：行点击 → `HistoryAction::Jump(index)`，关闭 → `Close` →
  `Message::History(...)`（窗口 Root 设置 `DialogResult::HistoryJump/Cancel`）。
- 验收：
  - `history_bands` 2/2：行点击（含未来步骤与初始状态）回传正确下标、
    关闭按钮回传 `Close`、计数与提示可访问；
  - `egui_chrome` 38/38：模型镜像（行数 / 当前 / 未来 / 计数 / 标题栏标记）
    与两个动作映射；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；`capture_iced` 18/18、
    `capture_egui` 12/12，基线零改动；
  - 真实窗口（ppp=1.25、Light）：工具栏时钟按钮打开「撤销历史」，顶部 iced
    标题栏可见；标题 / 提示 /「▶ 初始状态」当前行高亮 / 计数 / 主色关闭按钮
    与 iced 版本逐项一致；点「关闭」→ 日志 `egui 撤销历史动作：Close` 且
    对话框关闭。
  - 证据图：`target/hist3-dialog.png`、`hist2-dialog.png`（egui）、
    `hist-off-dialog.png`（iced 对照）。

**Task 1.3（第二十七步）：批量编辑对话框迁移到 egui + 模型拆分 — 已完成**

- **模型**：`BatchEditModel`（`titlebar` / 标题 / 「设置」「格式」小节标题 /
  四行输入 / 格式说明 / 按钮）+ `BatchEditRowModel { field, label,
  placeholder, value }` + `BatchEditFieldModel`（Velocity / Gate / Key / Tick）；
  `DialogModel::BatchEdit(Box<...>)`。
- **渲染**：`ui-egui/src/batch_edit.rs` 对齐生产：整窗 base 底、内边距 24、
  标题 18px、两张圆角 16 卡片（1px `strong` 边框、内边距 16）、
  行 = 固定 110 宽标签列 + 8 + 120×28 输入（`base` 底 + 1px `strong` 边框）、
  行间距 12，底部右对齐「确定」（主色）/「取消」（次要）。
- **真实窗口暴露并修复的两处错位**：
  - `add_sized(110, label)` 让标签在 110 宽里居中 → 与生产左对齐不符，且
    `allocate_ui_with_layout` 只按内容宽度前进、把输入框提前；改为
    `allocate_exact_size` 占满标签列后放子 `Ui`（左对齐）；
  - 输入框底色原为 `background.weak`，生产 `text_input` 外框实际是
    `background.base` —— 改正后卡片内输入框与 iced 一致。
- **标题栏让位**：与工程设置 / 历史一致，顶部 30px iced 自制标题栏继续由
  iced 绘制，`dialog_input_exclusive` 对 BatchEdit 在顶部区域不消费点击。
- **动作**：`InputChanged(field, text)` / `Confirm` / `Cancel` →
  `Message::BatchEdit(...)`（`Cancel` = `CloseDialog`）；表达式解析与范围
  校验仍由生产侧负责。
- **顺带拆分（REF-001，全部 < 400 行）**：
  - `egui_chrome/dialogs.rs` 363 行 → 抽出 `dialogs/batch_edit.rs`；
  - `chrome/model.rs` 552 行 → `model/{dialog,sidebar,chrome}.rs` +
    `model/{batch_edit,history}.rs`；
  - `egui_overlay.rs` 380 行 → 动作分发抽到 `egui_overlay/actions.rs`
    （`OverlayActions::take` + `apply_overlay_actions`），`draw` 返回值从
    5 元组收敛为 `(blocked, wants_keyboard)`。
- 验收：
  - `batch_edit_bands` 3/3：确定 / 取消按钮、力度输入回传
    `InputChanged(Velocity, "+20")`、小节标题与格式说明可访问；
  - `egui_chrome` 40/40：模型镜像（四行字段 / 占位 / 值 / 四行格式说明 /
    标题栏标记）与满载动作映射；
  - `ui-egui` 全量（含 history / project_settings / batch_edit / theme_palette /
    bands_pixels）、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；`capture_iced` 18/18、
    `capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真实窗口（ppp=1.25、Light，逻辑窗 318×614）：从卷帘右键菜单打开
    「批量编辑」，顶部 iced 标题栏可见；标题 / 两张卡片 / 四行标签与输入 /
    格式四行 / 底部按钮与 iced 版本逐项一致；力度输入 `+20` → 日志
    `egui 批量编辑动作：InputChanged(Velocity, "+20")`，点「确定」 →
    `Confirm` + `应用批量编辑: velocity=+20`，对话框关闭。
  - 证据图：`target/be-egui4.png`、`be-egui-edit.png`（egui，对话框窗口截图）、
    `be-off-dialog.png`（iced 对照）。

**Task 1.3（第二十八步）：变速对话框迁移到 egui — 已完成**

- **模型**：`SpeedChangeModel`（`titlebar` / 标题 / 倍率标签与占位 / 当前倍率 /
  提示 / 按钮）+ `DialogModel::SpeedChange(Box<...>)`。
- **渲染**：`ui-egui/src/speed_change.rs` 对齐生产：
  - 各区块按**固定纵向偏移**摆放（标题 0 / 倍率行 50 / 提示 96 / 按钮 145）——
    iced 的 `column.spacing(4)` 会叠加到每个 `space()` 上，等价于
    「title → 24 → row → 12 → hint → 24 → buttons」；egui 行高不同，
    用显式偏移才能得到同一套盒子位置；
  - 输入框 120×33、手绘外框（`base` 底 + 1px `strong` 边框、圆角 4）——
    `Frame` 会被 `TextEdit` 行高撑高（实测比生产高 5px），因此固定盒子 +
    内缩子 `Ui` 放文本；
  - 按钮 34 高、宽度 = 文本宽 + 48（生产 `padding([8,24])`）、间距 12。
- **真实窗口暴露并修复的三处偏差**：
  - 输入框底色原用 `background.weak`；生产 `text_input` 自身再画 base 底，
    实测可见底色是 **base**（白），已改正；
  - 输入框高度被 `TextEdit` 行高撑到 47 物理像素（生产 41），改手绘固定盒；
  - 提示文案原用 `text_muted`（#383838），生产 `strong.text` 实测最深像素为
    纯黑，改用正文色。
- **标题栏让位**：与其余对话框一致，顶部 30px iced 自制标题栏（红绿灯 / 拖动）
  仍由 iced 绘制，`dialog_input_exclusive` 对 SpeedChange 在该区域不消费点击。
- **动作**：`FactorChanged(text)` / `Confirm` / `Cancel` →
  `Message::SpeedChange(...)`（`Cancel` = `CloseDialog`）；小数 / 分数解析仍由
  生产侧负责。
- 验收：
  - `speed_change_bands` 3/3：确定 / 取消、倍率输入回传 `FactorChanged`、
    标题与提示可访问；
  - `egui_chrome` 42/42：模型镜像（倍率 / 标签 / 占位 / 提示 / 标题栏标记）
    与三个动作映射；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；`capture_iced` 18/18、
    `capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真实窗口（ppp=1.25、Light，Ctrl+点工具栏「变速」）：对话框 309×320
    与 iced 版本逐项对照——输入框盒子 149×40 vs 149×41（起点差 1px）、
    确定按钮 92×39 vs 94×41、提示行 198-212 vs 197-211、标题 72-93 vs 73-94；
    输入 `1/3` → 依次回传 `FactorChanged("1")/("1/")/("1/3")`，点确定 →
    `Confirm` + `应用音符变速: 倍率=0.33333334`，对话框关闭。
  - 证据图：`target/sc-dialog5.png`、`sc-edit.png`（egui）、
    `sc-dialog-iced.png`（iced 对照）、`sc-compare4.png`（并排）。

**Task 1.3（下一步）：自定义精度 / 导出进度对话框**

- 现状：保存确认 / 设置 / 工程设置 / 撤销历史 / 批量编辑 / 变速已是 egui；
  其余对话框仍由 iced 绘制。
- 下一步：按「自定义精度 → 导出进度」顺序迁移，每个沿用
  「模型 + 动作通道 + 输入独占 + band 测试 + 真实窗口」流程。
