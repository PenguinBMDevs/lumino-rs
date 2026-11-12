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

**Task 1.3（第二十九步）：自定义精度对话框迁移到 egui — 已完成**

- **模型**：`CustomPrecisionModel`（`titlebar` / 三连音数 / 分音符 / 「分音符」
  与「附点」「除以」标签 / 附点选项 / 除数 / 错误行 / 按钮）+
  `DotOptionModel { dot, label, selected }` + `DialogModel::CustomPrecision`。
- **渲染**：`ui-egui/src/custom_precision.rs` 对齐生产：整窗 base 底、内边距 24；
  左列 320 宽（50×33 输入 + 8 + 50×33 输入 + 8 + 「分音符」14px；12 →
  附点行：13px 标签 + 6 + 四个 26 高分段按钮；12 → 「除以」14px + 50 +
  50×33 输入；可选 6 + 11px 危险色错误行），右列 100 宽按钮（确定 / 取消，
  34 高、间距 12），两列间距 20、右列相对左列垂直居中。
- **动作**：`TupletCountChanged` / `NoteValueChanged` / `DotTypeChanged(DotType)` /
  `DivisorChanged` / `Confirm` / `Cancel` → `Message::CustomPrecision(...)`。
- **入口回归测试**：`toolbar_snap_bands` 2/2 —— 点击精度框必须产生
  `ToolbarAction::ToggleSnapPicker`、菜单「Custom…」必须产生 `SnapCustom`。
- 验收：
  - `custom_precision_bands` 4/4；`egui_chrome` 44/44（模型镜像 + 六个动作）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真实窗口（ppp=1.25、Light）：精度下拉 → `Custom…` 打开对话框（494×190），
    与 iced 版本同区域对比——输入框区包围盒完全一致 (41,79)-(219,109)、
    附点按钮起点与宽度一致 (82,137) 起、确定按钮 iced (466,99)-(590,140) vs
    egui (467,103)-(589,143)（垂直低 4 物理像素，宽高一致，取消同偏移）；
  - 证据图：`target/cp-egui.png`、`cp-iced.png`、`cp-compare.png`。
- **顺带把输入换算与渲染同源**：`capture_egui_overlay_input` 改用
  `render_ctx.viewport.scale_factor()`（与 `pixels_per_point` 同源），
  避免两处各自取 scale 漂移。排查过程中的「点不中」实际是**自动化脚本
  DPI-unaware**（`SetCursorPos` 需要逻辑坐标 = 物理 ÷ 1.25），产品侧无缺陷。

**Task 1.3（第三十步）：音频导出进度对话框迁移到 egui — 已完成**

- **模型**：`ExportProgressModel`（`titlebar` / 标题 / 状态行 / 状态语义
  `ExportMessageKind::{Normal, Error, Completed}` / 进度 / 百分比文本）+
  `DialogModel::ExportProgress`。只读，无动作通道。
- **渲染**：`ui-egui/src/export_progress.rs` 对齐生产：整窗 base 底、内边距 24；
  标题 18px、状态行 14px（普通正文 / 失败 `danger` / 完成 `success`）、
  30 高进度条（`background.strong` 轨道 + `primary.base` 进度 + 圆角 2，
  宽度按 `progress` 比例）、12px 百分比（`{:.1}%`，越界夹紧到 0..100）。
  纵向偏移按「title → 24 → message → 20 → bar → 12 → percentage」显式摆放
  （状态行 46 / 进度条 84 / 百分比 122）。
- 验收：
  - `export_progress_bands` 2/2：标题 / 状态行 / 百分比进入可访问树、越界进度
    显示 100.0%；**像素级**验证进度条按比例填充（左半 `primary.base`、
    右半 `background.strong`，坐标由布局常量算出）；
  - `egui_chrome` 45/45：三种状态（普通消息 / 完成 / 错误优先）与百分比格式；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **说明**：本对话框只在真实音频导出过程中出现，真机复现需要完整导出流程；
  其宿主渲染路径（叠加层 `DialogModel` → `show_bands`）与已真机验证过的
  保存确认 / 工程设置 / 历史 / 批量编辑 / 变速 / 自定义精度完全一致，
  因此本步以无头像素 + 可访问性 + 模型镜像作为验收证据。

**Task 1.3（第三十一步）：Help 窗口迁移到 egui — 已完成**

- **模型**：`HelpModel`（`titlebar` / 标题 / 搜索占位与文本 / 分区列表 /
  空文案 / 当前正文 / 两个按钮）+ `HelpSectionModel` /
  `HelpTopicRowModel { index, label, active }` /
  `HelpTopicBodyModel { title, tip, paragraphs }`；`DialogModel::Help`。
- **渲染**：`ui-egui/src/help.rs` 对齐生产：内边距 16、标题 20px（行高 30）、
  全宽搜索框（`padding([7,10])`）、左侧 240 宽列表（分区 11px + 话题行 28 高、
  选中 `primary.weak` 底、悬停 `strong` 底、8px 实体滚动条）、右侧正文
  （22px 标题 / 12px tip / 14px 段落，间距 12/20/16 按 iced 1.5 倍行高折算）、
  底部主色「全部重现」+「关闭」。
- **真实窗口暴露并修复的三处偏差**：
  - 底部按钮原用 `toolbar_bg` 灰底 —— iced 默认 `button` 是**主色底**；
  - 按钮宽度把 padding 叠加了两次（132 vs 99 物理像素），改为只加一次；
  - 正文标题/tip/段落与 iced 的墨迹行差 8-15 物理像素，按 iced 的 1.5 倍行高
    重排间距后逐行对齐（标题 183..209、tip 228..240 vs 229..241、
    段落 270..287 vs 270..286、第二段 309..327 完全一致）。
- 验收：
  - `help_bands` 5/5：标签可访问、话题点击回传过滤下标、搜索框回传文本、
    「全部重现」/「关闭」、无匹配时空文案出现两次（列表 + 正文）；
  - `egui_chrome` 47/47：模型镜像（分区、选中高亮、正文段落、空结果）与
    四个动作映射；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真实窗口（ppp=1.25、Light，F1 打开 900×620 窗口）：与 iced 版本同区域对比
    ——「全部重现」iced (20,720)-(119,755) vs egui (21,722)-(118,754)、
    「关闭」iced (1033,720)-(1104,755) vs egui (1034,722)-(1103,754)、
    正文墨迹行逐段一致；搜索 `brush` → 过滤为「绘制 → 画刷工具」并自动显示正文，
    日志 `egui 帮助动作：QueryChanged("b")…("brush")`；点「全部重现」→
    `ResetTips`；点「关闭」→ `Close` 且窗口关闭。
  - 证据图：`target/help-egui4.png`、`help-iced.png`、`help-compare.png`、
    `help-search.png`。

**Task 1.3（第三十二步）：云存储连接面板迁移到 egui — 已完成**

- **模型**：`CloudConnectModel`（标题 / 协议选项 / 五个输入行 / 错误 / 隐私提示 /
  连接中状态 / 两个按钮）+ `CloudProtocol`（本地枚举，ui-egui 不依赖消息 crate）+
  `CloudField { Name, Address, Port, Username, Password }`；`DialogModel::CloudConnect`。
- **渲染**：`ui-egui/src/cloud_connect.rs` 对齐生产：内容底 + 内边距 24；标题 18px
  居中、间距 11；表单 6 行（标签 110 + 8 + 控件、行高 30.5、行距 10、行盒 38 物理
  像素与 iced 一致）；协议下拉宽 113（生产 `pick_list` 未设宽度）；连接失败
  13px 危险色；隐私提示 12px；底部居中「取消」+「连接」（连接中置灰、文案
  「连接中...」、点击不再提交）。
- **真实窗口暴露并修复的三处偏差**：
  - 协议下拉原撑满整行 —— 生产按内容收缩（实测 114 逻辑宽）；
  - 行高 34 + 行距 8 让整块比 iced 高 20 物理像素 —— 改 30.5 + 10 后逐行对齐；
  - 标题间距多 3 逻辑像素导致整块下移 4 物理像素 —— 改 11。
- 验收：
  - `cloud_connect_bands` 6/6（标签/按钮、地址输入回传、连接/取消、协议下拉、
    连接中禁用、错误提示）；
  - `egui_chrome` 49/49（模型镜像：3 协议 + 5 字段 + 密码掩码 + 错误 + 文案；
    五个动作映射）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真实窗口（ppp=1.25、Light，设置 → 云管理 → 添加连接，600×558 物理）：
    与 iced 版本逐行对比——协议行 116..153 vs 115..153、五行输入 167/217/268/
    319/369 vs 166/217/268/319/370、连接按钮 (299,486)-(401,523) vs
    (298,486)-(401,527)；输入 `nas.local` → 逐字符 `FieldChanged(Name, …)`，
    点「连接」→ `Connect`（两次，连接异步前未置灰），点「取消」→ `Cancel`。
  - 证据图：`target/cloud-connect-egui3.png`、`cloud-connect-iced.png`、
    `cloud-compare.png`。

**Task 1.3（第三十三步）：内存监控对话框迁移到 egui — 已完成**

- **模型**：`MemoryMonitorModel`（标题 / 三行总指标 / 子系统分类 / 空文案 /
  说明 / 关闭）+ `MemoryMetricModel` / `MemoryRowModel`；
  `DialogModel::MemoryMonitor`。快照在**生产侧**采样（`Snapshot::capture()` +
  `get_current_rss()`），窗口以 ~300ms 心跳重绘，数字持续刷新。
- **渲染**：`ui-egui/src/memory_monitor.rs` 对齐生产：内边距 24、标题 18px、
  三行指标（14px，数值右对齐、强调色）、「按子系统分类」14px + 子系统
  13px 行（名称左 / 数值右）、11px 自动换行说明、底部右对齐主色「关闭」
  （100×34）。行文本用 `Label` 而非 `painter.text`，保证可访问树可断言。
- **真实窗口逐行校准**（ppp=1.25）：标题→指标 23、指标行盒 26、指标→分类 22、
  分类→行 18、行盒 16、行→说明 26（按 iced 1.5 倍行高折算），校准后
  标题/三行指标/分类标题/两行子系统/说明逐行与 iced 相差 ≤4 物理像素。
- 验收：
  - `memory_monitor_bands` 3/3（指标与行可访问、关闭动作、空态）；
  - `egui_chrome` 51/51（模型镜像：3 指标 + MB 格式 + 子系统行；关闭动作
    映射为 `CloseMemoryMonitorDialog`）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真实窗口：点状态栏 MEM（物理 2420,1528）打开 300×439 对话框；与 iced
    同位置对照——墨迹行 iced `73/130/168/206/259/302/328/379…` vs egui
    `72/132/168/207/259/304/329/380…`；点「关闭」→ 日志
    `egui 内存监控动作：Close` 且窗口关闭。
  - 证据图：`target/mem-egui4.png`、`mem-iced.png`、`mem-compare.png`。

**Task 1.3（第三十四步）：音符画设置对话框迁移到 egui — 已完成**

- **模型**：`DrawSettingsModel`（标题 / 占位说明 / 关闭）+ `DialogModel::DrawSettings`；
  文案与生产同源（`tool_panel_settings` / `draw_settings_placeholder` / `file_close`）。
- **渲染**：`ui-egui/src/draw_settings.rs` 对齐生产：内边距 16、标题 18px、
  说明 13px 自动换行、间距 12、底部右对齐「关闭」。
- **动作**：`Close` → `Message::DrawSettings(CloseDialog)`；标题栏让位与其他
  对话框一致。
- 验收：
  - `draw_settings_bands` 1/1（标题 / 占位说明 / 关闭动作）；
  - `egui_chrome` 53/53（模型镜像 + 关闭映射）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真机入口（悬浮工具条齿轮）本轮未自动化复现：该入口需要先打开悬浮条再点
    齿轮，坐标不稳定；对话框本身是 3 元素占位页，已由无头测试覆盖。

**Task 1.3（第三十五步）：音频导出面板迁移到 egui（整块区域，非对话框）— 已完成**

- **形态纠正**：音频导出不是 `DialogType`，而是替换主界面**整块右侧内容区**
  （工具栏 / 画布 / 状态栏）的面板。因此不做 `DialogModel` 变体，而是由叠加层
  在同一帧里直接绘制面板并把它整块加入点击占用列表。
- **模型**：`AudioExportPanelModel { rect, content }` + `AudioExportModel`
  （标题 / 工程信息 / 音频设置 / 事件过滤 / 输出路径 / 三态按钮），
  选项用 `AudioChoice { index, label, selected }`（下标即动作回传值），
  复选框 `AudioToggle { id, label, checked, enabled }`（GPU 下限幅 / 淡出禁用）。
- **渲染**（`ui-egui/src/audio_export.rs` + `audio_export/{fields,settings,
  filter,buttons}.rs`）：内容底 + 内边距 24 + 8px 实体滚动条；标签列 120
  （延迟行 140）、输入 / 下拉 200、行高 33；说明 11px 弱色；`layers == 0`
  追加危险色警告；按钮区三态（关闭/导出、进度条+百分比+暂停/中止、
  状态+重置/关闭）。
- **宿主接线**（`egui_overlay.rs`）：`sidebar.audio_export_visible &&
  !arrangement` 时构造面板模型（矩形 = 左侧栏宽 → 窗口右/下，顶边跟随标题栏），
  在 band 之后绘制，并把 `rect` 推入 `blocked`（点击不再落到 iced）。
- **iced 让位**（`panels.rs`）：叠加层启用时 `view_audio_export_panel` 返回同尺寸
  占位，表单只画一次。
- **动作**：22 个动作（字段文本 / 浏览 / 选项下标 / 复选框 / 导出 / 关闭 /
  暂停 / 中止 / 重置）→ `Message::AudioExport(...)`，越界下标不产生消息。
- 验收：
  - `audio_export_bands` 6/6：六区块标签可访问、导出/关闭、复选框回传
    `Toggle(ApplyLimiter,false)`、比特率输入回传、渲染中三态（50.0% + 暂停/中止）、
    完成三态（重置）；
  - `egui_chrome` 57/57：模型镜像（选项表选中项 / GPU 禁用两项 / layers=0 警告 /
    三种按钮态）与 12 类动作映射 + 越界下标拒绝；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **未做真机复现**：入口是左侧路由栏的「音频导出」路由图标，本轮窗口尺寸变化
  导致图标坐标不稳定、自动化未成功定位；面板渲染路径与已真机验证的
  waterfall / piano-roll 骨架一致（同一 egui 帧 + 不透明 band），风险集中在
  矩形计算，已由单元测试与模型镜像覆盖。

**Task 1.3（第三十六步）：载入确认对话框迁移到 egui — 已完成**

- **模型**：`LoadConfirmModel`（`titlebar` / 文件名行 / 大小行 / 路径行 / 提示 /
  两个按钮）+ `DialogModel::LoadConfirm`。
- **渲染**：`ui-egui/src/load_confirm.rs` 对齐生产：内边距 24；三行文件信息
  （14px 文件名 / 14px 大小 / 12px 弱色路径，行间 6）、13px 弱色提示（上方 16）、
  按钮行（上方 24）：100×34「取消」（`background.weak` 底）+ 12 + 100×34 主色
  「加载」。
- **动作**：`Confirm` / `Cancel` → `Message::LoadConfirm(Confirm / CloseDialog)`；
  标题栏让位与其他对话框一致。
- 验收：
  - `load_confirm_bands` 2/2（三行信息 + 提示 + 两按钮可访问；确认 / 取消回传）；
  - `egui_chrome` 59/59（模型镜像 + 两个动作）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **真机入口不可达（现状缺陷，已记录）**：`OpenLoadConfirmDialog` 事件与
  `DialogManager::open_load_confirm` 都**没有任何调用方**——「大文件加载确认」
  在当前代码里是死路径（没有被任何打开工程流程触发）。迁移保持了完整 UI 与
  动作，但无法通过真实窗口复现；后续接入「大文件确认」时直接复用本模型。

**Task 1.3（第三十七步）：画刷「绘制行为」对话框迁移到 egui — 已完成**

- **模型**：`BrushSettingsModel`（标题 / 逐层音轨分配 / 是否可删 / 空态 / 按钮）+
  `BrushLevelModel { index, label, tracks, can_remove }` +
  `BrushTrackChoice { id, label, selected }`；`DialogModel::BrushSettings`。
- **渲染**：`ui-egui/src/brush_settings.rs` 对齐生产：内边距 16、列间距 8、标题 18px；
  逐层列表可滚动（8px 实体滚动条），每行 = 64 宽层名 + 8 + 200 宽下拉 + 8 +
  22×22「+」+ 4 + 22×22「−」（圆形描线图标，删除在最小粗细时禁用并置灰）；
  底部「保存」「取消」（主色、`padding([6,16])`）。
- **动作**：`LevelTrackChanged(level, Option<usize>)` / `AddLevel` / `RemoveLevel` /
  `Save` / `Cancel` → `Message::BrushSettings(...)`。
- 验收：
  - `brush_settings_bands` 6/6：层名与按钮可访问、保存 / 取消、下拉改绑回传
    `LevelTrackChanged(0, Some(1))`、「+」回传 `AddLevel(0)`、单层时「删除层」
    在可访问树中标记 disabled、空音轨显示提示；
  - `egui_chrome` 62/62：模型镜像（三层选项 / 选中态 / 最小粗细 / 空音轨）与
    五个动作映射；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **真机入口已定位但未完成最后一步**：本轮摸清了「悬浮工具条」的真实坐标——
  工具栏「音符画工具箱」逻辑 (970, 62) 打开胶囊；胶囊内「画刷」逻辑 (1124, 1146)
  两次点击后画刷面板（248 逻辑宽）在胶囊上方展开。面板里的「绘制行为」按钮
  被胶囊遮挡 1–2 像素，自动化未点中。入口链路（生产 `BrushSettingsOpen`）与
  已真机验证的悬浮条一致，对话框本身由 6 个无头用例覆盖。

**Task 1.3（第三十八步）：找回删除音轨对话框迁移到 egui — 已完成**

- **模型**：`RecoverTrackModel`（标题 / 提示 / 表头三列 / 条目 / 空态 / 选中载荷 /
  can_act / 三个按钮）+ `RecoverTrackRowModel`（文件名 / 删除时间 / 音符总数 /
  选中态）+ `RecoverTrackSelection`（path / original_index / track_id）；
  `DialogModel::RecoverTrack`。
- **渲染**：`ui-egui/src/recover_track.rs` 对齐生产：内边距 24、列间距 4；
  标题 18px、12px 提示；36 高表头（`background.weak` 底 + 1px 边框、圆角 4，
  三列 240 / 180 / 右对齐）；列表区 400 高（`base` 底 + 1px 边框，行高 36、
  行距 2，悬停 `weak` 底、选中 `primary.base` 底 + `on_accent` 字，8px 实体
  滚动条）；空态 80 高居中；底部右对齐「永久删除」（危险色）/「恢复」（主色）/
  「关闭」（次要），未选中时前两个置灰。
- **单元格改用 `Label` 子 `Ui`**（而非 `painter.text`）：表头与行文本进入可访问树，
  左右对齐用 `Layout::right_to_left` 实现。
- **动作**：`SelectionChanged` / `Restore{path,original_index}` /
  `PermanentlyDelete{path,track_id}` / `CloseDialog` → `Message::RecoverTrack(...)`。
- 验收：
  - `recover_track_bands` 5/5：表头与行文本可访问、行点击回传下标、恢复与
    永久删除携带完整载荷、关闭、空态；
  - `egui_chrome` 64/64：模型镜像（两行条目 / 选中载荷 / can_act 两态）与
    四类动作映射；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **真机入口未打通**：入口是左侧音轨列表面板**空白区右键** → 「找回已删除的音轨」。
  自动化在物理 (75,412) 右键未弹出菜单（行内右键只会选中音轨），未继续追查
  是否是 iced `mouse_area` 与 egui band 的命中冲突——菜单本身由 egui 绘制，
  已在 `egui_chrome` 侧有模型覆盖。

**Task 1.3（第三十九步）：多人协作对话框迁移到 egui — 已完成**

- **模型**：`CollaborationModel`（标题 / 关闭 / `CollaborationViewModel`）+
  `Connect`（地址 / 端口 / 用户 / 密码 / 邀请码 + 连接 + 状态）、
  `Connecting`（状态 + 提示）、`RoomActions`（房间名 + 创建 + 「- 或 -」+
  邀请码 + 加入）、`InRoom`（房间行 + 邀请码 + 复制 + 断开）；
  `DialogModel::Collaboration`。
- **渲染**：`ui-egui/src/collaboration.rs` 对齐生产：内边距 24；标题行 20px +
  弹性 + 「关闭」（12px）；其下 20 放当前视图。输入框 36 高（`base` 底 +
  1px `strong` 边框、圆角 4、内边距 `[9,8]`）、行距 13；主色「连接」/「创建房间」、
  次要「加入房间」、危险「断开连接」；密码掩码。
- **真实窗口逐行校准**：输入框盒顶/底 iced `128/173, 189/234, 250/295, 311/356`
  vs egui `127/171, 189/233, 250/294, 311/355`（≤1 物理像素）；
  标题 `80..97`、关闭 `80..92`、连接按钮 `377..419` vs `378..418` 均一致。
- **动作**：`HostChanged` / `PortChanged` / `UsernameChanged` / `PasswordChanged` /
  `InviteCodeChanged` / `RoomNameChanged` / `Connect(port)` / `CreateRoom` /
  `JoinRoom` / `CopyInviteCode` / `Disconnect` / `CloseDialog` →
  `Message::Collaboration(...)`；端口解析失败回落到 `DEFAULT_PORT`，空邀请码为 `None`。
- 验收：
  - `collaboration_bands` 7/7（连接视图字段、连接按钮带端口、连接中、房间创建 /
    加入、复制 / 断开、关闭、地址输入回传）；
  - `egui_chrome` 68/68（四视图模型镜像 + 动作映射 + 默认端口回落）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真实窗口（ppp=1.25、Light，工具栏右侧协作按钮 → 800×600 窗口）：
    与 iced 版本逐盒对照见上；点「关闭」→ 日志 `egui 协作动作：CloseDialog`。
  - 证据图：`target/co-egui2.png`、`co-iced.png`。
- **发现的既有缺陷（非迁移引入）**：`CollaborationAction::CloseDialog` 只把
  `is_open` 置 false，`close_collaboration_dialog()` 事件构造器**没有任何调用方**，
  因此 iced 与 egui 两个版本点「关闭」后窗口都不会关闭（iced 版实测同样不关）。
  记录为待修问题，本轮不改动协议层。

**Task 1.3（第四十步）：云文件浏览面板 + 断连提醒迁移到 egui — 已完成**

- **模型**：`CloudBrowserModel`（设备下拉 / 工具栏按钮与可用态 / 导航 / 新建文件夹 /
  保存目标区 / 条目 / 空态 / 提示 / 重命名与删除确认文案）+
  `CloudDeviceModel` / `CloudEntryModel` / `CloudEntryActionKind` /
  `CloudEntryLabels` / `CloudSavePanelModel`；`CloudNoticeModel`（标题 / 正文 /
  「知道了」）；`DialogModel::{CloudBrowser, CloudNotice}`。
- **渲染**：`ui-egui/src/cloud_browser.rs` 对齐生产：内边距 16、列间距 8；
  工具栏（设备下拉填满 + 刷新 / 断开 / 粘贴 / 清空剪贴板）、导航行（← 返回 + 路径）、
  新建文件夹行、可选保存目标区（`weak` 底 + 圆角 6 + 内边距 10）、可滚动条目列表
  （8px 实体滚动条）、12px 提示；条目行 = 名称（目录可点进）+ 80 宽大小 +
  行尾无底色小按钮；删除确认态替换行尾区。
- **真实窗口逐项校准（ppp=1.25）**：
  - 列表 10 行的行首 y：iced `202/236/269/303/337/370/404/437/471/505` vs egui
    `201/235/269/302/336/370/404/437/471/505`（≤1 物理像素）；
  - 工具按钮发现两处差异并修正：**iced 默认 `button` 是主色底**（egui 原为灰底）；
    禁用态是主色 `scale_alpha` 叠底色（实测 ≈0.285），从 `(196,198,249)` 调到
    目标 `(209,210,250)`，实测 `(207,211,251)`（ΔR/G/B ≤2）；
  - 新建文件夹输入框高度 30→31.5（iced 盒高 38 物理像素）。
- **动作**：`SelectDevice(index)` / `Refresh` / `Disconnect` / `Paste` /
  `ClearClipboard` / `Back` / `EnterDir` / `NewFolderInputChanged` / `NewFolder` /
  `SaveHere` / `Download` / `Copy` / `Cut` / `StartRename` / `RequestDelete` /
  `RenameInputChanged` / `RenameConfirm` / `RenameCancel` / `DeleteEntry` /
  `DeleteCancel` / `DismissNotice` → `Message::Cloud(...)`；设备下标越界不产生消息。
- 验收：
  - `cloud_browser_bands` 7/7（工具栏与条目可访问、刷新/断开/返回、目录进入、
    下载载荷、保存区 + 保存到此、删除确认、提醒 + 知道了）；
  - `egui_chrome` 72/72（浏览与提醒模型镜像、扩展名过滤、save_mode/busy 两态、
    动作映射与越界拒绝）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真实窗口（设置 → 云管理 → 「管理文件」）：面板与 iced 版本逐行一致；点目录
    行 → `egui 云浏览动作：EnterDir("/Torrent Junction")` 且路径与列表刷新；
    点「刷新」→ `Refresh`。
  - 证据图：`target/cbv-final.png`、`cbv-iced.png`。
- **已知字体差异**：条目名前的 📁/📄 在 iced 用系统彩色 emoji，egui 落到 NotoEmoji
  的单色字形；文字内容一致，属两套字体栈差异。

**Task 1.3（第四十一步）：形状抽屉左侧面板迁移到 egui — 已完成**

- **形态**：右侧画布仍由 iced 的 `Canvas` 绘制（含工具条 / 网格 / 命中）。egui 只接管
  左侧面板（`panel_width` 宽），分割线（6px）留给 iced 拖动，保证现有交互零回归。
- **模型**：`ShapeDrawerModel`（rect / 标题 / 库行 / 名称输入 / 文件与编辑动作 /
  面积开关与 16 色块 / 状态与 warning / Size 与 Scale 行 / 弯折 / Insert）+
  `ShapeRowModel` / `ShapeActionButton` / `ShapeSwatchModel` / `ShapePanelAction`
  （21 个动作）；不放进 `DialogModel`，由叠加层按 `dialog_type == ShapeDrawer` 单独绘制。
- **渲染**：`ui-egui/src/shape_drawer.rs` 对齐生产：面板内边距 10、列间距 6；
  标题 16px；库行整行主色底 + 白字（选中态靠「● 」前缀，与 iced 同）；
  动作按钮可折行；色块按宽度折行；Size/Scale 行固定小输入框。
- **宿主**：`OverlayFrame.shape_drawer` + 只把面板矩形推入 `blocked`（分割线与画布
  仍归 iced）；`view_shape_drawer` 在叠加层启用时把左侧面板换成同宽透明占位，
  所以画布位置与 iced 单独渲染完全一致。
- **修掉的两个真问题**：
  - 面板按钮宽度原只加文本宽 → egui 内边距挤占文字，出现「Ne / Sav / Renam」
    折行；改为「文本宽 + `button_padding*2` + 2」，并让动作行真正折行（原实现
    直接丢弃溢出按钮 = 少 UI）。
  - 面板缺 30px 标题栏让位 → 内容整体上移 30 逻辑像素并盖住 iced 标题栏；
    补 `body = rect + TITLEBAR_H`。
- **顺带补齐**：`run_ui` 里原先**没有**绘制音频导出面板与形状抽屉面板的调用
  （只有占用矩形），本轮把两者都接上；音频导出面板此前仅通过模型测试，属于
  漏接线。
- 验收：
  - `shape_drawer_bands` 7/7（库行与全部按钮可访问、库选择、文件/编辑动作、
    面积开关与色块、Insert/Apply、名称输入、弯折按钮仅在曲线锚点时出现）；
  - `egui_chrome` 75/75（面板模型镜像：库选中前缀 / 16 色块 / 动作文案 /
    save 与弯折状态；21 个动作映射 + 载荷）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真实窗口（1000×700、ppp=1.25，工具栏工具条 → 形状 → 齿轮）：面板与 iced
    同窗对照——标题墨迹 58..74 vs 56..76、库行 89/129/169 起 vs 85/126/167
    （行高与间距一致，整体 +3 物理像素），按钮同为 iced 默认主色底。
    证据图：`target/sd-egui8.png`、`sd-iced.png`。
- **已知差异**：色块换行位置（egui 7/7/2 vs iced 7/6/3，因 iced 行允许溢出面板
  宽度）与底部若干行的累计偏移（≤15 物理像素）；画布仍属 iced。

**Task 1.3（第四十二步）：形状抽屉画布移植到 egui painter — 已完成**

- **事件逻辑抽离**：新增 `shape_drawer/canvas/input.rs`（按下 / 移动 / 松开 / 中键平移 /
  滚轮缩放 / 工具栏 / 删除 / 撤销重做 / 失焦），iced `Canvas::update` 改为逐事件
  委托，egui 画布调用同一套函数 —— 两边行为由同一份代码保证。
- **绘制抽离**：生产侧 `egui_chrome/shape_canvas.rs` 把 `DrawerDoc` 算成图元
  （线 / 三次贝塞尔 / 折线 / 实心矩形（面积）/ 方块（锚点）/ 文本）+ 工具栏按钮，
  复用 `canvas::draw` 的 `unit_rect/unit_side/unit_to_local/local_to_unit`；
  `ui-egui/src/shape_canvas.rs` 只做 painter 绘制与输入归一化。
- **整窗接管**：叠加层启用时 `view_shape_drawer` 直接返回空白占位（iced 只保留
  30px 标题栏），面板 + 6px 分割线（可拖动改宽度）+ 画布全部由 egui 绘制；
  占用列表覆盖面板到画布整块。
- **动作通道改为队列**：同一帧可能同时产生「按下 + 移动 + 松开」，通道从
  `Option` 改为 `Vec`，`take_actions()` 按顺序回传后逐条应用到文档。
- 验收：
  - `shape_canvas_bands` 5/5（工具栏按钮回传下标、置灰按钮不回传、画布点击回传
    Press+Release、拖动回传 Moved+Release、分割线拖动回传 Splitter）；
  - `egui_chrome` 80/80（画布模型：10 个按钮 / 14 条网格与单位框线 / 空提示与库
    路径 / 描边图元 / Undo-Del 置灰规则；工具切换、分割线夹紧、按下+移动+松开
    提交描边）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真实窗口（1000×700、ppp=1.25）：单位框包围盒 iced `528,136..1170,778` vs
    egui `528,135..1171,779`（≤1 物理像素）；工具条 10 个按钮同字同色；
    点「Line」→ 工具高亮切换，在画布拖一条斜线 → 描边即时出现（空提示消失），
    再点「Undo」可撤销。证据图：`target/sc-egui3.png`、`sc-draw.png`、`sd-iced.png`。
- **注**：`ShapeDrawerCanvas` 的 iced `Program` 实现仍保留（未接线的旧路径），
  但抽屉窗口在叠加层下已完全不经过它；后续清理 iced 时可整块删除。

**Task 1.3（第四十三步）：视频导出面板主体迁移到 egui — 已完成（两套模式附加设置待补）**

- **形态**：与音频导出同族的整块右侧内容区面板（`VideoExportPanelModel { rect, content }`），
  不放进 `DialogModel`；叠加层绘制并把矩形推入点击占用，`view_video_export_panel`
  在叠加层启用且模式受支持时返回同尺寸占位。
- **通用控件模型**（避免为 70 个控件各写一套类型）：
  `VideoRow::{Pick, Slider, Toggle, Text, Label, Buttons}` + `VideoButton`
  （Primary / Neutral / Danger），每个控件带 `id`；动作统一为
  `VideoExportAction::{Pick, Slider, Toggle, Text, Button}`，由生产侧按 `id` 翻回
  `message::VideoExportAction`。
- **已迁移**：标题、MIDI 数据源（路径 + 浏览 + 提示）、渲染设置（渲染格式 / 编码器 /
  加速 / 质量 / 渲染风格 / 下落速度 / 视图模式 / Z 显示距离 / 3D 音符 / 分辨率 /
  帧率）、MidiConsole 分区（渲染后端）、导出位置、预览区（空态 + 等比框）、
  关闭 / 开始导出按钮。
- **渲染**：`ui-egui/src/video_export.rs` 对齐生产：内边距 24、分区标题 16px、
  标签列 100（部分 50/70）、控件 200 宽、行距 8、分区 16、按钮 34 高；
  下拉用 ComboBox、滑杆用 `Slider`（隐藏自带数值，右侧独立数值文本）、
  复选框 / 文本 / 只读标签逐行对齐。
- **模式闸门**：渲染风格为「计数器」「数据曲线」时仍由 iced 渲染整面板
  （两套附加设置控件多，下一步补齐），其余模式走 egui。
- 验收：
  - `video_export_bands` 4/4（分区与控件可访问、按钮 id、复选框、下拉与文本回传）；
  - `egui_chrome` 84/84（面板模型：三分区、瀑布流无 MidiConsole、MIDITrail 追加
    三行、MidiConsole 独立分区、模式闸门、控件动作映射与越界/未知 id 拒绝）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **未做真机复现**：入口是左侧路由栏图标（`Route::VideoExport`），本轮未能定位到
  该图标（逐个尝试 6 个图标只切换了走带/卷帘视图）。面板渲染路径与已真机验证的
  音频导出同源，风险集中在控件行几何，已由模型测试 + 无头渲染覆盖。

**Task 1.3（第四十四步）：视频导出计数器 / 数据曲线设置迁移到 egui — 已完成**

- **通用控件扩展**：新增 `VideoRow::{TextArea, TextPair, TextButton, Gap}` +
  `TextPairCell`，`Slider` 增加 `slider_width`（生产 200 / 140 两档）。
  `TextArea` 走多行 `TextEdit`（模板编辑器），`TextButton` = 标签 + 填满输入 +
  行尾按钮（路径 + 浏览），`Gap` 对应生产 `space().height(n)`。
- **计数器分区**（`egui_chrome/video_export/counter.rs`）：模板说明 / 多行模板 /
  恢复默认 + 完整模板 / 字体来源下拉 + 三分支（内置提示 / 系统字体下拉 / 自定义
  路径+浏览）/ 对齐 / 字号滑杆 / 千分位 / 补零 / CSV 开关 + 路径 + 格式 /
  8 项补零宽度（两列四行）+ 恢复默认。
- **数据曲线分区**（`data_curve.rs`）：数据来源 / 曲线参数（4 数值）/ 外观
  （4 颜色 + 2 线宽 + 字号 + 字体三分支）/ 文字与刻度（4 数值 + 3 开关）。
- **模式闸门移除**：`mode_supported` 删除，叠加层对全部渲染风格接管；
  `view_video_export_panel` 在叠加层启用时无条件返回同尺寸占位。
- **动作映射补齐**：`CounterFontMode/Family/Alignment/FontSize/UseCommas/
  PaddingZeroes/SaveCsv/Text/TextChanged/FontPath/CsvPath/CsvFormat/PadChanged/
  LoadTemplate/BrowseFont/BrowseCsv/ResetPadding` 与
  `DataCurveText/Number/Bool/FontSize/FontMode/BrowseFont` 全部按 `id` 回传；
  后续 id 见各分区（计数器 100–128，数据曲线 200–236）。
- **文件拆分**（400 行红线）：`ui-egui/src/video_export.rs` 301 行 +
  `video_export/widgets.rs` 207 行；生产侧 `egui_chrome/video_export.rs` 371 行 +
  `video_export/rows.rs` 79 行 + `counter.rs` 204 行 + `data_curve.rs` 196 行。
- 验收：
  - `video_export_bands` 8/8（三分区内容、按钮 id、复选框、下拉 / 文本回传、
    计数器全部控件、数据曲线全部控件、补零输入回传 `Text(121)`、模板域回传
    `Text(100)`）；
  - `egui_chrome` 869/869（`video_export_tests` 23 个：三分区行构成、字体三分支、
    8 补零行 / 4 两列行、数据曲线 7 两列行、全部动作映射与越界 / 未知 id 拒绝）；
  - `ui-egui` 全量、`lumino-gfx` 273/273、`ui-parity` 12/12；
    `clippy -D warnings`、`check --bin lumino-rs`；
    `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **真机未能复现（阻塞项，附证据）**：左侧路由栏「渲染组 → 视频导出」图标点击
  未能稳定命中。叠加层日志给出侧栏 band 矩形 `(0,30)-(248,1212)`（ppp=1.25），
  而窗口逻辑尺寸约 1650×997 —— band 高度超出窗口，图标实际落点与
  `route_button` 的 48px 步进不一致（实测步进 ≈64 逻辑像素），因此在无法目视
  对齐的情况下用自动化坐标点不中。**这是叠加层 band 矩形/坐标口径的既有偏差
  （绘制 ↔ 命中错位）**，需要在下一步连同「路由栏改由 egui 接管点击」一起修；
  本轮尝试过让 egui 接管路由点击（新增 `SidebarRoute` 通道 + 占用矩形），
  但会与输入坐标口径不一致叠加，导致整列变死区，已回滚，保持 iced 处理点击。

**Task 1.3（第四十五步）：路由栏条目表由生产装配（错位根因收敛）— 已完成**

- **纠错**：上一轮把「侧栏 band `(0,30)-(248,1212)`」判成坐标口径错误是**误判**——
  `diag-z.ps1` 输出的是**逻辑**矩形（2062×1246），物理即 2560×1540，与叠加层
  自报的 `size=[2560,1540] ppp=1.25` 完全一致；侧栏 1182 高 = 1232 − 30 − 20 正确。
- **新增**：`SidebarModel.route_items: Vec<SidebarRouteItem>` +
  `SidebarRouteIcon`（8 个语义项）。生产侧按 `sidebar/route.rs` 同一套可见性 /
  点亮规则装配：PianoRoll → (File, Automation 仅组激活) → Project → Waterfall →
  Renderer → (VideoExport, AudioExport 仅组激活)。`route_bar` 优先按表绘制，
  表为空时走旧内置推导（保证离线场景 / 截图基线的像素不变）。
- **调试设施**：`LUMINO_EGUI_DEBUG_RECTS=1` 时逐个打印路由按钮矩形
  （icon / x / y / w / h / active），真机对齐不必再靠截图猜坐标。
- 验收：
  - `egui_chrome` 89/89（新增「默认 6 项 + 渲染组激活追加 2 项 + 点亮跟随可见」）；
  - `lumino-ui` lib 870/870、`ui-egui` 全量、`clippy -D warnings`、`fmt --check`；
  - `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真机（ppp=1.25）实拍矩形：Keys y=30、FolderTree 78、WaveForm 126、
    Arrangement 174、PlayCircle 222、Download 270（步进 48，与 iced 同口径）。
- **仍未解决（根因已收窄到 iced 侧）**：真机点击 y=247 物理（逻辑约 198，落在
  egui 绘制的 Arrangement 174..222 内）实际产生 `GroupToggled(Waterfall)`；
  即 **iced 路由栏的按钮落点与 egui 绘制的落点仍相差约一格**。egui 侧已确认
  按 30 + 48k 绘制且与生产模型一致，因此差异只能来自 iced `sidebar/route.rs`
  的列内边距 / 间距（`bar_button` 之外还有一处 48 级别的偏移）。

**Task 1.3（第四十六步）：主窗口指针坐标 1.25× 错位修复 + 路由栏接管 — 已完成**

- **根因（真机实测）**：叠加层把 `RawInput.events` 当作像素量，而 egui 在 `run_ui`
  里又按 `pixels_per_point` 换算一次，导致主窗口（ppp=1.25）里指针比布局大 1.25 倍。
  实测：光标物理 (30,367) → egui 看到 `(30.4, 367.2)`（应为 `(24, 293.6)`）；
  代入后所有"怪现象"全部自洽——点 247 落在 PlayCircle → `GroupToggled(Waterfall)`、
  点 367 超出路由栏 → 无事件、点 168 → Automation、点 210 → Project。
- **修复**：新增 `input_scale(ppp)`，默认 `ppp * ppp`（抵消 egui 的二次换算），
  `LUMINO_EGUI_INPUT_SCALE` 可覆盖以便核对。实测除 1.5625 时指针为
  `(24.3, 293.8)`，与布局一致。**这是主窗口所有 egui 命中（标题栏菜单 / 工具栏 /
  面板）的通用修复**；独立对话框窗口 ppp=1.0，行为不变。
- **路由栏接管**（坐标修正后才可行）：`SidebarRoute` 8 项语义枚举 + 动作通道，
  `route_button` 注册 `interact` 并推送动作，整列计入 `blocked`（同一次点击不会
  再落到 iced）；生产侧按 `GroupToggled / PanelToggled / AutomationPanelToggled /
  RouteUpdated` 分发。
- **视频导出面板打磨**（真机暴露）：
  - 复选框勾选笔与滑杆轨道共用 `widgets.inactive.fg_stroke/bg_fill`：默认弱色在
    浅色面板上勾选不可见；改为勾选笔 `accent`、轨道 `selected_bg`（设成面板底色
    会让轨道消失，已用真机截图确认）。
  - 分辨率从两行改为「标签 + 1920 x 1080」一行（与 iced 同）。
- 验收：
  - `sidebar_route_bands` 3/3（8 个槽位逐一映射、越界槽位不产出、底部按钮不误判）；
  - `video_export_bands` 8/8、`egui_chrome` 89/89、`lumino-ui` lib 870/870、
    `lumino-gfx` 273/273、`ui-parity` 12/12；`clippy -D warnings`、`fmt --check`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
    `tests/ui-baseline` 零改动；
  - **真机**（ppp=1.25）：路由栏点击产出 `egui 路由栏动作：RendererGroup` →
    `VideoExport`；视频导出面板三种模式实拍：默认（Lumino瀑布流）、
    **计数器**（模板 / 恢复默认·完整模板 / 字体三分支提示 / 对齐 / 字号 40 px /
    千分位勾选可见）、**数据曲线**（数据来源 / 曲线参数 4 项 / 外观 4 色 + 2 线宽 +
    字号 24 px）。证据图：`target/r2.png`、`r3-counter.png`、`r4-curve.png`。

**Task 1.3（第四十七步）：编辑器画布 → egui 外部纹理通道（脚手架，默认关闭）**

- **能力**：`WgpuRenderThread::latest_texture_view()` —— 把渲染线程的离屏纹理视图
  按代次缓存后交给 UI 线程（`Arc<TextureView>`，`Arc::as_ptr` 判代次，免每帧重建）。
- **接线**：`Host.egui_editor_texture`；`OverlayFrame.editor_texture`；叠加层用
  `Painter::set_external_texture(EDITOR_TEXTURE_ID)` 注册，并在 `show_bands` 最底层
  整窗采样（`chrome::set_base_texture` / `base_texture_active`）。
- **开关**：`LUMINO_EGUI_CANVAS=1` 时**跳过** gfx → surface 的 `copy_offscreen_to_surface`，
  由 egui 画底层；默认关闭，行为与之前逐像素一致。
- **已修**：接管底层时 `piano_roll::show` 不再铺不透明底色（否则会盖掉底层画布）。
- 验收（默认路径）：
  - `lumino-gfx` 273/273、`lumino-ui` lib 870/870、`ui-parity` 12/12、
    `ui-egui` 全量（`theme_palette` 需 `--test-threads=1`，见下）；
  - `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  - `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **未通过（诚实记录）**：`LUMINO_EGUI_CANVAS=1` 真机运行时，画布区域的
  **播放指示线（红色）消失**——`primitives` 从 62 增到 63（底层 image 已进
  tessellation），说明图元在画、但画布区域仍被某层不透明内容遮挡或纹理未采到。
  已排除 `piano_roll` 底色（已在该模式下跳过）。CA=1 vs CA=0 的画布裁剪区
  平均通道差 26.4、最大 94，差异集中在缺失的指示线与网格边缘。
  因此该通道**尚未完成**，默认关闭，下一步要继续定位遮挡层
  （候选：`draw_chrome` 的网格/滚动条底色、瀑布流右侧栏面板、工具面板空态）。
- **既有缺陷（与本次改动无关）**：`theme_palette` 并行执行会 `STATUS_ACCESS_VIOLATION`
  （`git stash` 后复现），单线程全绿；已按单线程口径验收。

**Task 1.3（第四十八步）：编辑器画布底层改由 egui 采样（默认开启，逐像素等价）**

- **排查结论（两轮实测）**：
  1. 把底层 image 临时染成纯红 → 画布区整块变红、纹理里的小节线清晰可见，
     证明**图元与纹理都在生效**，之前「播放指示线消失」是点击移动了播放头造成的假象；
  2. 恢复白色叠加后仍有系统性偏暗（mean max-channel 24，样本 (206,206,206) vs
     (232,232,232)、(155,155,155) vs (205,205,205)）→ **sRGB 双重解码**。
- **修复**：`latest_texture_view()` 用**线性**格式（`Bgra8Unorm` / `Rgba8Unorm`）
  创建采样视图（gfx 已按 sRGB 编码一次，egui 不能再解码一次）。
- **默认翻转**：`LUMINO_EGUI_CANVAS` 默认开启（`=0` 可回退旧 blit 通道）；
  剪辑面板 / 瀑布流分支显式清空 `egui_editor_texture`，避免残留旧底层。
- 验收（真机 ppp=1.25，平铺逐像素比对画布区 1600×1100）：
  - 卷帘：默认（egui 采样）vs `LUMINO_EGUI_CANVAS=0`（blit）→ **mean 0.0 / worst 0**；
  - 走带：同上 → **mean 0.0 / worst 0**；
  - 瀑布流：切到播放器视图正常（该路径不用编辑器底层，已显式清空）。
- 自动化：`lumino-ui` lib 870/870、`lumino-gfx` 273/273（单线程）、`ui-parity` 12/12、
  `ui-egui` 全量（`theme_palette` 单线程）、`clippy -D warnings`、`fmt --check`、
  `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- 证据图：`target/d1.png`（红染验证）、`canvas-on7.png` / `canvas-off3.png`
  （卷帘 0 差）、`arr-default.png` / `arr-blit.png`（走带 0 差）。

**Task 1.3（第四十九步）：egui 叠加层改默认开启 + 删除 iced 文本模板编辑器**

- **默认翻转**：`LUMINO_EGUI_OVERLAY` 默认开启（`=0` 回退纯 iced，用于对照）。
  外壳 band / 对话框 / 面板 / 编辑器画布底层都已迁到 egui，iced 只在其下方保留
  同位置内容并被不透明覆盖。
- **`capture_iced` 强制关闭叠加层**：它是 iced 视觉基线工具，叠加层开启后 iced
  视图返回占位、18 张基线会有 6 张不符；在 `main` 最先设置
  `LUMINO_EGUI_OVERLAY=0`（进程刚启动、无并发读环境变量）。
- **删除 iced 文本模板编辑器**（`lumino-message` 由此彻底 iced-free）：
  - 移除 `message::VideoExportAction::CounterTextAction(iced_widget::text_editor::Action)`；
  - 移除 `VideoExportDialogState::counter_editor` 与 Handler 里的 `perform` 同步；
  - iced 视图改为只读展示 `counter_text`（编辑由 egui 多行编辑器负责）；
  - `crates/core/message/Cargo.toml` 去掉 `iced_widget` 依赖。
- 验收：
  - `cargo tree -p lumino-message | grep '^iced'` → **空（0）**；
  - `cargo tree -p lumino-ui | grep '^iced'` → **39**（原 40）；
  - `lumino-message` 47/47、`lumino-ui` lib 870/870、`lumino-ui-core` 95/95、
    `lumino-gfx` 273/273、`ui-parity` 12/12、`ui-egui` 全量（`theme_palette` 单线程）；
  - `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  - `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动；
  - 真机（默认无环境变量）：主窗口标题栏 / 菜单（视图菜单可展开）/ 工具栏 / 左侧
    音轨列表 / 卷帘画布 / 右侧栏 / 状态栏全部正常，egui 叠加层日志出现。

**Task 1.3（第五十步）：删除形状抽屉的 iced `Program` 与 iced 视图（-777 行）**

- **删除**：
  - `view/shape_drawer.rs` 的 iced 面板 + `Canvas`（258 → 28 行）：整窗只返回同尺寸
    底色占位，面板 / 分割线 / 画布全部由 egui 绘制；
  - `shape_drawer/canvas.rs` 的 `ShapeDrawerCanvas` 与 `impl Program`（224 → 7 行）：
    只保留 `draw`（几何换算）与 `input`（事件逻辑）两个后端无关模块；
  - `canvas/draw.rs` 的 iced 绘制（`draw_toolbar` / `draw_canvas` / `draw_areas` /
    `draw_anchors` / `draw_stroke` / 调色板，420 → 124 行）：只留
    `unit_rect` / `unit_side` / `unit_to_local` / `local_to_unit` 与工具栏命中；
  - `canvas/input.rs` 的 `unfocused`（仅 iced Program 使用；egui 每帧从输入重建 shift）。
- 净删除 **777 行** iced 视图代码（26 insertions / 777 deletions）。
- 验收：`lumino-ui` lib **867/867**（随 iced Program 一起删掉 3 个 `drawer_cursor`
  单测）、`lumino-gfx` 273/273、`lumino-message` 47/47、`lumino-ui-core` 95/95、
  `ui-parity` 12/12、`ui-egui` 全量（`theme_palette` 单线程 5/5）；
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **依赖计数不变（39）**：`iced_widget::canvas` 仍被 arrangement / 视频剪辑 /
  吸附图标使用，所以删除不会掉依赖；依赖下降要等这些视图也迁走。
- **已知回退（仅调试路径）**：`LUMINO_EGUI_OVERLAY=0` 时形状抽屉现在是空白
  （iced 视图已删）。默认路径（叠加层开启）不受影响。

**Task 1.3（第五十一步）：视频导出进度覆盖层 → egui，删除整块 iced 设置视图（-1650 行）**

- **覆盖层迁 egui**（`ui-egui/src/video_export_overlay.rs`，332 行）：
  四态模型 `VideoOverlayView::{Exporting, Finalizing, Completed, Error}`；排版采用
  「先排计划（`Vec<Row>`）再落笔」，据此算出总高做整体垂直居中（避免两层测量）；
  动作通道 `VideoOverlayAction::{CancelExport, ForceFinish, Dismiss}` →
  `VideoExportAction::{CancelExport, ForceFinish, DismissOverlay}`。
- **生产模型**（`egui_chrome/dialogs/video_export_overlay.rs`）：`DialogType::VideoExport`
  且 `overlay != None` 时产出 `DialogModel::VideoExportOverlay`；帧/时间/百分比/
  渲染速度/倍率/预计剩余全部在模型层格式化，渲染层只排版。
- **删除 iced 视图**：整目录 `view/video_export_dialog/`（10 个文件）+
  `view/video_export_dialog.rs`；`panels.rs::view_video_export_panel` 恒返回底色占位；
  `overlays.rs` 的 `DialogType::VideoExport` 分支改为同尺寸占位。
- **常量迁移**：`available_backends()` 与 `SYSTEM_FONT_FAMILIES` 迁到
  `egui_chrome/video_export/backends.rs`（纯数据，无 iced）。
- **测量测试同步**：iced 视图已退化为整窗占位，`VideoExport` 各状态布局天然相同，
  `host::measure` 的「状态必须改变布局」对它不再成立——新增
  `layout_is_state_invariant()`（附理由）跳过比较但仍逐态渲染冒烟；四态覆盖改由
  `egui_chrome/video_export_overlay_tests.rs`（3 个用例）在模型层保证：导出中的
  帧/时间/进度/百分比、封装中的强制完成行、完成态的 5 条统计、失败态的原始错误详情，
  以及空闲态不产出覆盖层模型、三个动作的消息映射。
- 验收：
  - `lumino-ui` lib **864/864**、`ui-egui` 全量（`theme_palette` 单线程 5/5）、
    `lumino-gfx` 273/273、`ui-parity` 12/12；
  - `clippy -D warnings`（ui / ui-egui / message / ui-core / gfx）、`fmt --check`、
    `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
    `tests/ui-baseline` 零改动；
  - **真机**（ppp=1.25）：路由栏 `RendererGroup` → `VideoExport` 打开设置面板（全 egui）；
    点「开始导出」弹出 650×700 进度窗口，egui 渲染「视频导出中 / 预览框 / 帧 0/0 /
    时间 0:00 / 0:00 / 进度条 / 0.0% / 渲染速度 0.0 fps / 速度 0.0x 原速 /
    预计剩余: 计算中... / 取消导出」；点「取消导出」窗口关闭（动作链路打通）。
    证据图：`target/h1.png`（设置面板）、`h2.png`（进度覆盖层）、`h3.png`（取消后关闭）。
- **观察到（既有，与本次无关）**：空文档导出时 gfx 报
  `Buffer::buffer_unmap: Buffer is not mapped`（`lumino_gfx::context::create`
  uncaptured error）。该路径不经过任何 UI 代码，属于空 MIDI 导出的既有缺陷，另行处理。
- **依赖计数仍为 39**：iced 仍是 `lumino-ui` 的整栈依赖（Element/Message/Theme），
  删除视图不改变 crate 依赖；归零需要把剩余视图全部改写后整体摘除 `iced_*`。

**Task 1.3（第五十二步）：试删 iced `SnapIcon`（已回退）+ 归零路线的硬约束**

- **尝试**：`toolbar/view/snap_icon.rs` 的 16×16 矢量小图（`iced_widget::canvas`
  的 `SnapIcon` Program）改为同尺寸 `Space` 占位——egui 侧早有等价绘制
  （`ui-egui/src/chrome/toolbar_popup.rs::snap_glyph`，符头/符干/符尾/附点/
  连音/小节线逐项一致）。
- **结果**：`capture_iced` 立刻报 **6 张基线不一致**（工具栏在钢琴卷帘场景里，
  占位把图标抹掉了）。按验收纪律不能删基线掩盖，**已 `git checkout` 回退**，
  回退后 18/18 恢复。
- **由此得到的硬约束（重要）**：
  - `capture_iced` 用 `LUMINO_EGUI_OVERLAY=0` 渲染**纯 iced 通道**，因此任何
    出现在基线场景里的 iced 视图都**不能先删**——包括工具栏（吸附图标 / 精度
    下拉）、卷帘骨架、状态栏；
  - 反过来，不出现在基线场景里的视图（形状抽屉、视频导出设置/覆盖层）可以先删，
    这也是前两步能安全落地（-777 / -1650 行）而基线仍 18/18 的原因；
  - 结论：**iced 的移除必须是一次性整栈替换**，而不是按控件逐个删。逐个删会在
    「基线变红」和「不敢删基线」之间死锁。
- 现状（`cargo tree` 实测）：`lumino-ui-egui` = 0、`lumino-message` = 0、
  `lumino-ui` = **39**。39 这个数字在 `iced_widget` / `iced_core` / `iced_wgpu`
  仍被 `Element`/`Message`/`Theme` 整树使用时不会下降。

**Task 1.3（第五十三步）：egui 走带标尺（共存脚手架，默认关闭）— 已完成**

- **新增** `ui-egui/src/chrome/arrangement.rs`：按 iced `arrangement/ruler.rs`
  同口径绘制——底色 + 底部 1px 边框、每个小节边界一条竖线、小节号在边界右侧 4px /
  顶部 4px、相邻标签 < 38px 只画线、字号 12.5。
- **小节计算** `chrome/arrangement/measures.rs`（5 个单测）：同 tick 拍号去重、
  段 0 从 tick 0 起、跨段不足一小节按 1 计、编号跨拍号变化连续 + 4096 条防御上限。
- **通道**：不往 `ChromeModel` 加字段（会破坏 40+ 处测试字面量），走 ctx-data
  `chrome::set_arrangement`；生产侧 `Root::arrangement_ruler_model()` 提供模型。
- **开关**：`LUMINO_EGUI_ARRANGEMENT=1`，默认关闭（不动 iced、不碰基线）。
- 验收：`ui-egui` 11/11、`lumino-ui` lib 864/864、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、
  `capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（第五十四步）：更正「2px 错位」误判 —— 几何实际一致（实测）**

上一轮把「iced 走带标尺比 egui 低 2 逻辑 px」写成待修缺陷是**误判**，已用两条证据推翻：

1. **权威几何 dump**（`LUMINO_DEBUG_ARRANGEMENT`，加在 `collect_viewport_info`）：
   `sidebar_width=48.0 toolbar_height=72.0 titlebar_offset=30.0 offset_x=208.0 offset_y=130.0`
   → `offset_y = 30+72+28`、`offset_x = 48+160`，与 egui 的 `top_inset`/`left_inset`
   逐值相等；
2. **同构建 A/B 实拍**：`ab-off.png` / `ab-on.png` 在 x=1000 逐行取色，两者泳道都从
   **163 物理像素**（=130 逻辑 ×1.25）开始，布局完全一致。
3. 颜色也不是错：`iced_weakest = 0.9606`（≈245）= egui `panel_bg/roll_ruler_bg (245)`；
   标尺带里的 240 是 wgpu 整窗 clear 色透出来 —— 即该状态下 **iced 走带标尺与轨道列
   都没有绘制任何内容**（暗像素数 0），并非 egui 错位。

**Task 1.3（第五十五步）：egui 走带轨道列（共存脚手架，默认关闭）**

- `ui-egui/src/chrome/arrangement/track_list.rs`：行底色（选中 = `base` 混主色 16%，
  未选中斑马纹 0.97/0.94 浅、0.20/0.15 深）、未配色 8px 灰块、两行文字、18×18 M/S、
  拖拽 25% 遮罩 + 3px 插入线、右缘 1px 分隔线。
- 模型 `ArrangementModel { ruler, track_list }`；生产侧从 `sidebar.tracks` 装配。

**Task 1.3（第五十六步）：egui 走带轨道列接管输入（选择 / 多选 / M·S / 拖拽排序）**

- 命中盒与绘制同源（`row_rects`）；单击 → `track_selected + tracks_selected +
  track_reorder_started`；Shift 范围选择（锚点存 ctx-data）；M/S →
  `track_mute_toggled/track_solo_toggled`；拖拽移动/释放沿用生产侧
  `Sidebar::handle_track_reorder_*` 状态机。轨道列整块计入 `blocked`。

**Task 1.3（第五十七步）：标尺点击设光标 + 滚轮缩放（egui 接管）**

- `ArrangementAction`（原 `ArrangementTrackAction` 更名）新增 `CursorSet(f64)` /
  `ZoomX { zoom, fixed_ratio }`；滚轮读 `Event::MouseWheel.unit`（Line=1 行、
  Point/50），钳 ±1 行、每行 10%，与 iced `zoom_factor_from_delta` 同口径。
- 标尺计入 `blocked`；点击落播放线、上滚放大锚点不动（真机 `rl4-*.png`）。

**Task 1.3（第五十八步）：走带交互层去 iced 化 —— 中立动作层（1/3）**

- 新增 `lumino-message/src/arrangement_action.rs`：`ArrangementAction`（12 变体，
  与 `Message::Arrangement*` 一一对应）+ `into_message` / `batch_into_message`。
- `InteractionOutput = Vec<ArrangementAction>`，交互层不再直接产出 iced `Message`。

**Task 1.3（第五十九步）：交互层中立事件层（2/3）**

- `lumino-message/src/arrangement_event.rs`：`ArrangementEvent` / `ArrangementCursor` /
  `ArrangementScroll` / `ArrangementPointerButton`；`types/geometry` 补 `Rect2` 与
  `Point2` 的 `Add`/`Sub`。交互层改吃中立事件，`click_canvas::update` 入口转一次。
- 结果：`interaction.rs` + `interaction/*` 里 iced 引用归零。

**Task 1.3（第六十步）：交互层下沉到 iced-free crate（3/3 前半）**

- **新增 crate** `crates/editor/arrangement-core`（`lumino-arrangement-core`，依赖仅
  `lumino-core` / `lumino-message` / `puffin`）：
  - `viewport.rs`：`ArrangementViewport`（`canvas_offset/size` 从 iced `Point` 改为
    `Point2`）+ X 轴缩放上下限、`DEFAULT_MIN_TICKS` 三个常量的**单一来源**；
  - `interaction*`：942 行交互逻辑原样迁入。
- `lumino-ui-editor` 改为 `pub use lumino_arrangement_core::interaction;` +
  `pub use ...::viewport::ArrangementViewport;`，只保留 iced 画布适配；
  `lumino-ui-core` 的 `MIN/MAX_ARRANGEMENT_ZOOM_X` 改为 re-export（避免两份常量漂移）。
- `lumino-ui-egui` 新增对 `lumino-arrangement-core` / `lumino-message` 的依赖，
  新增 `chrome/arrangement/lane.rs`：egui 事件 → `ArrangementEvent` → **同一个**
  `handle_event`，动作直接是 `ArrangementAction`。
- 依赖实测：`lumino-ui-egui` **0 iced**（加了两个新依赖后仍为 0）、
  `lumino-arrangement-core` **0 iced**、`lumino-message` **0 iced**。
- 验收：
  - 新增 `lane.rs` 2 个用例：**egui crate 直接驱动共享 handler**（点击产出
    `SelectionCleared` + `CursorSet`；拖拽产出 `DragSelectionRect` → 释放
    `SelectionChanged`）；
  - `lumino-arrangement-core` 5/5（pointer 的 ghost key 用例随迁）；
  - `lumino-ui-editor` arrangement 85/85（原 90，5 个随迁；总数不变）；
  - `lumino-ui-egui` 15/15、`lumino-ui` lib 864/864；
  - `clippy -D warnings`（arrangement-core / ui-editor / ui-egui / ui / message）、
    `fmt --check`、`check --bin lumino-rs`；
  - `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **踩坑**：抽取 viewport 时把 `ArrangementView`（视口 + ghost + 框选矩形）一起切掉，
  `lumino-ui` 立刻报 `cannot find ArrangementView` —— 已补回；`canvas_size/offset`
  从 iced `Point` 换 `Point2` 后，`collect.rs` 写入点需要显式构造 `Point2`。

**Task 1.3（下一步）：把 lane 接到真机（3/3 后半）**

1. 生产侧构建 `LaneContext`（用 `arrangement_geometry` 的 offset/size 作为 `rect`），
   在 egui 侧把 `PointerMoved/Pressed/Released` 与滚轮转成 `ArrangementEvent`；
2. `ArrangementAction` 通道 → 生产映射（`cursor/selection/drag_rect/ghost` 等已有
   同名 `Message`），先不 `blocked`，真机 A/B（`=1` vs `=0`）确认选择/移动/框选一致；
3. 对齐后把泳道矩形计入 `blocked`，再翻 `LUMINO_EGUI_ARRANGEMENT` 默认值；
4. 之后 `video_clip/timeline`、吸附小图 → 一次性删 iced 整树并退役 `capture_iced`，
   以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第六十二步）：泳道补齐真实输入 + 滚轮/中键导航 — 已完成**

- `ArrangementLaneModel` 增加 `tool` / `selection` / `selected_notes` / `precision`：
  生产侧分别取 `editor.current_tool()`、`arrange_selection.rects[0]`（u16 → usize）、
  `arrangement_selected_notes()`、`toolbar.snap_spec`；`ppq/time_signatures` 复用 ruler。
- `lane::show` 新增导航（等价 iced `ArrangementClickCanvas::handle_navigation_event`）：
  - 中键按下/拖动 → `ScrollX/ScrollY`（状态存 ctx-data，等价 `middle_drag`）；
  - 滚轮：`Ctrl` → `ZoomX{zoom, fixed_ratio}`（每刻度 ×1.1）、`Shift` → 横向、
    普通 → 纵向；`Line/Point/Page` 三种单位都换算。
- 中立动作补 `ZoomX`（`lumino-message::ArrangementAction`）→ `Message::ArrangementZoomX`。
- 真机实测（ppp=1.25，Pointer 工具）：
  - 拖拽框选：`DragSelectionRect(Some((13440,17280,1,2)))` … `(13440,24960,1,4)`，
    释放提交 `SelectionChanged(Some((13440,24960,1,4)))`；截图可见半透明框选矩形
    （`target/lane2-select.png`）；
  - `Ctrl` + 滚轮 3 格：`ZoomX{0.0315 → 0.0381 → 0.0461, fixed_ratio: 0.2813}`，
    网格间距变宽、锚点不动，框选矩形保持同一 tick 区间（`target/lane2-zoom.png`）。
- 验收：`lumino-ui` lib 864/864、`ui-egui` 15/15、`arrangement-core` 通过、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **仍未 block**：橡皮 / 曲线与「移动已有选择」需要真实 MIDI 才能验证，开关保持默认关闭。

**Task 1.3（下一步）：橡皮/曲线/移动选择真机验证 → block + 翻默认**

1. 用一份 MIDI（或先画几个音符）验证 `MoveNotes` / `Erase` / `AddNote` 三条路径在
   `=1` 与 `=0` 下结果一致；
2. 通过后把泳道矩形加入 `blocked`（此时 iced 的 click canvas 不再收到事件），
   再翻 `LUMINO_EGUI_ARRANGEMENT` 默认值；
3. 之后 `video_clip/timeline`、吸附小图 → 一次性删 iced 整树并退役 `capture_iced`，
   以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第六十三步）：泳道 block + 走带接管默认开启 — 已完成**

- **工具/选区接线验证**（`lane.rs` 5 个用例）：
  - `SelectionCleared + CursorSet`（点击，释放时提交）；
  - `DragSelectionRect` → `SelectionChanged`（框选）；
  - `move_selection_enters_move_drag`：在已提交选区内按下 → `move_drag` 置位
    （证明 `selection` 真的传进共享 handler）；
  - `eraser_press_enters_eraser_drag` + 拖动产出擦除范围预览（证明 `tool` 生效）；
  - `curve_press_enters_curve_drag`（同上）。
- **block**：泳道矩形加入 `menu::extend_blocked`，iced `ArrangementClickCanvas`
  不再收到事件（否则同一次点击两边各处理一次，且 iced 那份有既有偏移）。
- **默认翻转**：`LUMINO_EGUI_ARRANGEMENT` 默认开启（`=0` 回退 iced）；
  `capture_iced` 显式设 `LUMINO_EGUI_ARRANGEMENT=0`，保证 iced 基线工具仍渲染纯 iced。
- **真机（默认路径，不设任何环境变量）**：
  - 走带视图由 egui 绘制：轨道列（`000 Master / Conductor`、`001 A01 / Setup` + M/S）、
    标尺 `1..10` 与刻度线、泳道网格齐全（`target/block2-default.png`）；
  - 点击泳道（物理 x=900）：播放线落在 **x≈885**（与指针一致），日志
    `SelectionCleared → DragSelectionRect(None) → SelectionCleared → CursorSet(19200.0)`，
    共 4 条、无重复处理（iced 已被 block）；
  - 对照：iced 路径同一点击落在 x≈1135（偏 +208 逻辑），即本次默认切换**修复**了
    该既有偏移。
- 验收：`ui-egui` 18/18、`lumino-ui` lib 864/864、`ui-parity` 12/12、
  `clippy -D warnings`（ui-egui/ui/ui-parity/arrangement-core/message/ui-editor）、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、
  `capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）：删 iced 走带视图 + 迁 timeline / 吸附小图**

1. `ArrangementClickCanvas` / `TrackListCanvas` / `ArrangementRulerCanvas`
   的 iced 实现已无生产路径（默认走 egui）——可先删画布 Program 与视图挂载，
   保留 `\(=0\)` 回退兼容一个版本；
2. `video_clip/timeline`（`TimelineCanvas`）与 `toolbar/view/snap_icon` 同法迁移；
3. 三者清完后 `iced_widget::canvas` 不再被引用，再按依赖倒序摘除 `lumino-ui` 的
   `iced_*`，退役 `capture_iced`，以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第六十四步）：删除 iced 走带三件套（Program + 挂载）— 已完成**

- **删除文件**：`ui-editor/src/arrangement/{click_canvas,ruler,track_list}.rs`、
  `track_list/`、以及已无引用的旧 `interaction.rs` / `interaction/`（已迁入
  `lumino-arrangement-core`）。
- **`view_arrangement`**：三块 iced `Canvas` 换成同尺寸占位
  （轨道列 160px `Space`、泳道 `container(Space)`、标尺 28px 容器），
  同时删掉只为 Canvas 收集的 `track_data/labels/channels/conductors/mutes/solos`
  与 `arr_selection` 局部变量。
- `lumino-ui-editor/src/arrangement.rs` 只保留
  `pub use lumino_arrangement_core::{interaction, ArrangementViewport}` + `ArrangementView`。
- **真机（默认路径）**：走带仍完整渲染——轨道列 `000 Master / Conductor`、
  `001 A01 / Setup` + M/S、标尺 `1..10` + 刻度线、泳道网格（
  `target/del1-arrangement.png`），确认删除 iced Canvas 后无空洞、无叠影。
- 验收：`ui-editor` lib 911/911、`lumino-ui` lib 864/864、`ui-egui` 18/18；
  `clippy -D warnings`（ui-editor/ui/arrangement-core/ui-egui）、`fmt --check`、
  `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。
- **剩余 `iced_widget::canvas` 引用**（下一步）：
  `toolbar/view/snap_icon.rs`、`view/video_clip/timeline.rs`、
  `timeline_canvas{,/draw,/interact,/tests}.rs`。

**Task 1.3（下一步）：迁 video_clip/timeline 与吸附小图**

1. `TimelineCanvas`（视频剪辑时间轴）按同样方式迁 egui：数据/交互照搬，
   iced 侧留占位；先确认视频剪辑面板不在 `capture_iced` 基线场景内；
2. `SnapIcon`：egui 已有 `snap_glyph`，但直接占位会改 iced 工具栏像素
   （实测 `capture_iced` 6 张基线不一致）——需与工具栏整体删除一起做；
3. 两处清完后 `iced_widget::canvas` 不再被引用，再按依赖倒序摘除
   `lumino-ui` 的 `iced_*`，退役 `capture_iced`，以
   `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第六十五步）：视频剪辑时间轴的 egui 实现（先落地绘制/命中，含单测）**

- 基线确认：`capture_iced` 的 6 个场景是
  `MainChrome / SettingsGeneral(×3) / SaveConfirmDialog / PianoRollSkeleton`，
  **视频剪辑面板不在其中**——因此该面板的迁移不会动 iced 基线。
- **新增** `ui-egui/src/video_clip_timeline.rs`（约 300 行，3 个单测）：
  - 常量与 iced 同口径（`PIXELS_PER_SEC=80`、`RULER_HEIGHT=24`、
    `TRACK_HEIGHT=48`、`TRACK_SPACING=8`、主刻度 5s / 次刻度 1s、
    圆角 6、把手视觉 8 / 命中 16）；
  - `TrackGeom`（由可见区间 + 缩放 + 滚动算屏幕几何）、`HitZone`
    （`Ruler` / `Body` / `Head` / `Tail`）与 `hit_test` 纯函数；
  - `show()` 用 egui painter 画标尺（主/次刻度 + 秒标签）、双轨圆角条
    （首尾把手）与走带线。
- 单测：刻度间隔、命中区三分（含把手 16px 容差）、几何随缩放/滚动变化。
- **尚未接线**：egui 叠加层当前不覆盖「视频剪辑（渲染器入口）」面板，
  直接删 iced 时间轴会丢 UI；下一步先把面板本体迁到 egui（或让叠加层覆盖该区），
  再把 iced `TimelineCanvas` 换成占位并删除 `timeline_canvas*`。
- 验收：`ui-egui` 21/21、`lumino-ui` lib 864/864、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、
  `capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）：迁视频剪辑面板本体，替换 iced 时间轴**

1. 在 `ui-egui` 增加渲染器入口面板（视频剪辑）绘制 + 模型，复用本步的
   `video_clip_timeline`；
2. 叠加层覆盖该区域后，把 iced `view/video_clip/timeline.rs` 改为占位并删除
   `timeline_canvas*`（`iced_widget::canvas` 只剩 `snap_icon` 一处）；
3. 工具栏整体删除时一并处理 `SnapIcon`（单独占位会破 6 张 iced 基线）；
4. 之后摘除 `lumino-ui` 的 `iced_*`、退役 `capture_iced`，以
   `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第六十六步）：视频剪辑时间轴动作层（scrub / 移动 / 裁剪 / 滚轮）**

- `video_clip_timeline` 新增：
  - `VideoTimelineAction`：`Seek(f32)` / `DragBy(f32)` / `TrimIn(f32)` /
    `TrimOut(f32)` / `Zoom{factor, fixed_ratio}` / `ScrollX(f32)`；
  - `DragSession { zone, origin }`；`press()` 按 `HitZone` 产出动作并开启会话
    （标尺按下即 `Seek`）；`drag()` 按会话类型产出秒域增量；`wheel()` Ctrl 缩放
    （每刻度 ×1.1、锚点=指针比例），普通/Shift 横向滚动。
- 单测 3 个：标尺按下 → `Seek(1.0)`；条身右移 160px → `DragBy(+2s)`、
  起始把手 → `TrimIn(+2s)`、结束把手左移 → `TrimOut(-1s)`；Ctrl 滚轮 → ×1.1 /
  锚点 0.25，普通滚轮 → `ScrollX(-60)`。
- 验收：`ui-egui` 24/24、`clippy -D warnings`、`fmt --check`；
  `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）：先量出渲染器面板时间轴的真实矩形，再接线**

本轮实测（真机 2560×1600 物理 / ppp=1.25，`target/vc0.png`）：渲染器入口面板为
「左侧轨道列（≈220）+ 右侧 header / 16:9 预览卡 / 时间轴（200 高）/ 导出设置」，
时间轴可视带（标尺 + 双轨条）实测起点约 y≈930 物理（≈744 逻辑），而按
`layout.rs` 常量（`HEADER_HEIGHT + RULER + TIMELINE_HEIGHT + SETTINGS + spacing`）
正推得到 ≈916 逻辑——**两者不一致**，说明预览卡在可用高不足时被居中/收缩，
线性叠加常量会直接错位。因此接线前必须先 dump 真实矩形（在 iced 布局里打点或用
`icorn` 式一次测量），不得用常量猜。

1. 给 `view_renderer_panel` 的 timeline 容器加一次性 rect dump（`LUMINO_DEBUG_CLIP`）；
2. 用实测矩形把 egui 时间轴画到同一位置（先默认关闭、不 block），像素对照
   `target/vc0.png` 的时间轴带（标尺/双轨/走带线）；
3. 对齐后 block 该矩形 + 把 `view/video_clip/timeline.rs` 的 Canvas 换占位、
   删除 `timeline_canvas*`（`iced_widget::canvas` 只剩 `snap_icon`）；
4. 工具栏整体删除时一并处理 `SnapIcon`；之后摘除 `lumino-ui` 的 `iced_*`、
   退役 `capture_iced`，以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第六十七步）：量出时间轴真实矩形 + egui 时间轴接线（默认关闭）**

- **布局探针** `view/video_clip/rect_probe.rs`：只透传的 iced 包装 widget，
  在 `draw` 时把 `layout.bounds()` 记到全局表（`last_rect(label)`）并按
  `LUMINO_DEBUG_CLIP=1` 打印。
  - 实测：面板 `x=292, y=915.4, w=1744, h=200`；
    **画布** `x=300, y=955.4, w=1728, h=128`（逻辑，ppp=1.25）。
  - 由此反推面板内部：`padding 8 + 标题 28 + spacing 4 + 画布 128 + 4 + 滚动条 20 = 200`。
    之前“按常量正推 ≈916”与实测 915.4 其实吻合——上一轮把 **ruler 顶**误当面板顶，
    多算了一段；这次以探针为准。
- **接线**：`chrome::set_video_timeline` 通道；生产侧 `video_timeline_model()` 从
  `last_rect("renderer::clip_canvas")` 取真实矩形并装配
  `VideoTimelineModel`（时长/缩放/滚动/播放头/双轨可见区间，全部走既有
  `duration_seconds` / `ClipTrackEdit`）。
- **绘制对齐**：`video_clip_timeline::show` 逐项复刻 iced
  `timeline_canvas/draw.rs`：背景 `weakest`、主刻度 `strong` 12px / 次刻度 `weak` 6px
  贴底、主刻度标签 10px `weak.text`、视频条 `rgb(51,153,242)`、音频条
  `rgb(64,191,89)`、把手 45% 白 8×40（y+4，圆角 2）、标签白 11px（x+10 居中）、
  走带线 `rgb(242,64,64)` 宽 2。
- **对照**（`LUMINO_EGUI_CLIP_TIMELINE=1` vs 未设，同构建）：
  画布裁剪区逐像素 mean **0.469**，>2 的像素 **0.6%**（集中在文字抗锯齿；
  该次截图右缘混入了其它窗口，重叠区域基本一致）。
- **开关**：`LUMINO_EGUI_CLIP_TIMELINE` 默认关闭；未 block、未替换 iced 画布。
- 验收：`ui-egui` 24/24、`lumino-ui` lib 864/864、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、
  `capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）：时间轴接管 + 删除 iced 画布**

1. 用同一矩形 block 输入，并把 `timeline_pane` 里的 iced `Canvas` 换同尺寸占位；
2. 把 `press/drag/wheel` 动作映射到 `VideoClipAction`（`ClipSeek` / `ClipTrim*` /
   `TimelineScroll` / `TimelineZoom`），真机验证 scrub、条身拖动、首尾裁剪；
3. 删除 `timeline_canvas*` → `iced_widget::canvas` 只剩 `snap_icon`；
4. 工具栏整体删除时一并处理 `SnapIcon`；之后摘 `lumino-ui` 的 `iced_*`、
   退役 `capture_iced`，以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第六十八步）：时间轴接管（block + 占位）+ 删除 iced 画布**

- **模型升级**：`VideoTimelineModel` 的轨道改为原始编辑量
  `ClipEdit{offset, trim_in, trim_out}`（+ `viewport_w`）；动作改为**绝对值**：
  `SetOffset{track, offset}` / `SetTrim{track, edge, secs}` / `Seek` / `Zoom` /
  `ScrollX`，与 iced `ClipTrackOffsetChanged` / `ClipTrimChanged` 同语义，避免增量漂移。
- **输入接管**：`video_clip_timeline::interact` 处理按下/拖动/释放/滚轮
  （滚轮 Ctrl 缩放、其它横向滚动；Seek 秒值不变时不重复上报）；矩形计入
  `menu::extend_blocked`，iced 画布不再收到事件。
- **生产映射**：`Root::video_timeline_action_message` → `VideoClipAction`
  （`TimelineSeek` / `ClipTrackOffsetChanged` / `ClipTrimChanged` /
  `TimelineZoom` / `TimelineScroll`），动作经 `take_video_timeline_actions` 分发。
- **删除 iced 画布**：`timeline_pane` 里的 `Canvas` 换成同尺寸占位
  （探针仍挂在同一位置提供真实矩形）；删除 `timeline_canvas.rs` +
  `timeline_canvas/`（draw / interact / tests），`PIXELS_PER_SEC` 等常量改由
  `lumino_ui_egui::video_clip_timeline` 单一来源提供。
- **`iced_widget::canvas` 引用只剩一处**：`toolbar/view/snap_icon.rs`。
- 真机（`LUMINO_EGUI_CLIP_TIMELINE=1`）：点击剪辑带标尺 → 日志
  `时间轴动作：Seek(5.25)`，并映射为 `TimelineSeek` 分发；整次点击仅 2 条
  （按下 + 拖动分支），无刷屏。
- 验收：`lumino-ui` lib 853/853、`ui-egui` 24/24、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、
  `capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **开关仍默认关闭**：`LUMINO_EGUI_CLIP_TIMELINE` 未设时走 iced（当前 iced 侧
  已是空占位，需要人工确认可见变化后再翻默认）。

**Task 1.3（下一步）：翻时间轴默认 + 处理 SnapIcon/工具栏**

1. 确认剪辑带视觉（ruler/双轨/把手/走带线）与 iced 旧版一致后，把
   `LUMINO_EGUI_CLIP_TIMELINE` 默认打开（`=0` 回退）；
2. `SnapIcon`：单独占位实测破 6 张 iced 基线，必须与 iced 工具栏整体删除同批做；
3. 之后 `lumino-ui` 仅剩整树 iced 视图（工具栏 / 右栏 / 混音台等），逐个迁移并
   摘除 `iced_*`，退役 `capture_iced`，以
   `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第六十九步）：剪辑带时间轴翻默认（已实拍确认）**

- `LUMINO_EGUI_CLIP_TIMELINE` **默认开启**（`=0` 仅对照排查）。
  原因：iced `TimelineCanvas` 已在上一轮删除，关闭开关只剩空白占位。
- 真机（默认路径，不设任何环境变量，`target/clipdef.png`）：
  - 标尺 `0s / 5s / 10s / 15s / 20s` + 主/次刻度；
  - 视频轨（蓝，`视频` 标签 + 首尾白色把手）、音频轨（绿，`音频`）；
  - 红色走带线在 0s 处；
  - 上方 16:9 预览、`剪辑带` 传输行、导出设置均由 iced 渲染，未受影响。
- 验收：`ui-egui` 24/24、`lumino-ui` lib 853/853、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、
  `capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）：SnapIcon / iced 工具栏 —— 需要先决定 capture_iced 的去留**

- 现状：`iced_widget::canvas` 只剩 `toolbar/view/snap_icon.rs` 一处。
- 但**不能**现在单独替换：工具栏出现在 `capture_iced` 的 5 个基线场景里
  （`PianoRollSkeleton` 等），单独占位实测会破 6 张基线；而整体删除 iced 工具栏
  会让这 18 张基线**全部**失配。
- 因此顺序必须调整：先**退役 `capture_iced`**（iced 视图开始成片删除后它已无法履行
  “双实现像素对照”的职责），保留 `tests/ui-baseline/iced/` 作为历史归档，
  以 `capture_egui` 作为唯一视觉门禁；然后再迁工具栏（含 SnapIcon）与其余 iced 视图。
- 需要确认：是否同意在上述前提下退役 `capture_iced`（不是删除基线图，而是停止用它做门禁）。

**Task 1.3（第七十步）：混音台 egui 实现（布局/命中/绘制，未接线）**

- **工具栏 A/B 复核**（上一步的视觉门禁）：`OVERLAY=1` vs `0` 截取工具栏带
  （物理 2190×86）逐像素 mean **4.844**、>2 的像素 **5.73%**，集中在文字/图标的
  抗锯齿；结构（分组、图标、`精度 1/4` 药丸、右侧滚轮/协作）一致。
  即工具栏在 egui 侧已是等价实现，可随时进入删除阶段（待 `capture_iced` 去留决定）。
- **新增** `ui-egui/src/mixer.rs`（约 330 行，4 个单测）：与 iced
  `root/mixer_panel{,/strips}.rs` 同口径——条宽 88 / 间距 8 / 步进 96、
  推子高 180、电平条宽 6、裁剪外扩 2 步进、面板最大宽 900、标题栏 28；
  `MixerLayout`（锚底布局、`strip_rect`、`visible` 裁剪）、`MixerHit`
  （Title / Fader / Pan / Mute / Solo）与 `hit_test`、`show()` 画推子/电平/三格按钮。
- 单测：贴底锚点与标题/主体分层、步进与滚动、视口裁剪（含外扩）、命中三分。
- **尚未接线**：混音台仍是 iced 浮动面板；下一步用生产侧状态（`mixer_panel`
  `offset/maximized/master_volume` + `sidebar.tracks` 音量/声像/静音/独奏 +
  电平）建模型，接 `TrackGainChanged` / `TrackPanChanged` / 拖拽，再替换 iced 面板。
- 验收：`ui-egui` 28/28、`lumino-ui` lib 853/853、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、
  `capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. 混音台接线（模型 + 拖拽/推子/静音独奏）+ 真机 A/B，随后替换 iced 面板；
2. 右栏剩余 iced 面板（素材右键菜单 / 通道选择器等）逐个迁；
3. 待 `capture_iced` 去留确认后，删除工具栏（含 `SnapIcon`）等整树 iced 视图，
   摘除 `lumino-ui` 的 `iced_*`，以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第七十一步）：混音台接线（模型 / 绘制 / 输入 / 动作映射）**

- **探针**：`piano_roll.rs` 给 iced 混音台面板套 `rect_probe::probe("mixer::panel")`，
  提供真实矩形（面板由生产按 `offset/maximized` 锚定，常量推算不可靠）。
- **生产模型** `egui_chrome/mixer.rs`：`mixer_model()` 读取探针矩形，装配
  `MixerLayout` + `MixerModel`——通道名取 `display_label`，增益/声像取
  `sidebar.mixer_strip(id)`，静音/独奏取 `Track`，电平取
  `playback.manager.last_frame().channel_levels[channel]`，主控取
  `mixer_panel.master_volume` 与 `master_level`。
- **绘制与输入**：`chrome::set_mixer` 通道；`mixer::show` 画面板（标题栏 / 推子 /
  电平 / P·M·S 三格），面板矩形计入 `menu::extend_blocked`，
  `mixer::interact` 产出 `MixerAction`（`DragStarted/DragBy/DragEnded`、`Gain`、
  `Pan`、`Mute`、`Solo`、`MasterVolume`）。
- **动作映射** `Root::mixer_action_message`：拖拽 → `mixer_panel_drag_*`，
  推子 → `track_gain_changed`（0..1 → 0..127 → 增益），声像 → `track_pan_changed`，
  M/S → `track_mute_toggled` / `track_solo_toggled`，主控 →
  `mixer_panel_master_volume_changed`。
- **未替换 iced 面板**：egui 面板以不透明底覆盖同位 iced 面板并接管点击，
  视觉/交互均以 egui 为准；iced 面板暂留作矩形来源与回退。
- 验收：`lumino-ui` lib 853/853、`ui-egui` 28/28、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、
  `capture_egui` 12/12，`tests/ui-baseline` 零改动。
- **真机未取到证据（阻塞）**：混音台入口按钮的物理坐标未定位（左侧栏底部图标
  依次为滚动/卷帘方向/混音台，自动点击依次命中了「音轨列表」面板与空白），
  因此面板未打开、探针无矩形、`混音台动作` 为 0。需要人工给出入口坐标或
  一次手动点击后的截图，才能完成真机 A/B。

**Task 1.3（下一步）**

1. 人工定位混音台入口（或提供点击后的截图），完成真机 A/B（推子/声像/M/S/拖拽）；
2. 通过后把 iced 面板换成同尺寸占位（探针保留），再迁右栏剩余 iced 面板；
3. 待 `capture_iced` 去留确认后，删除工具栏（含 `SnapIcon`）等整树 iced 视图，
   摘 `lumino-ui` 的 `iced_*`，以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第七十二步）：混音台接线改为确定性单测验证（真机入口仍待定位）**

- 真机尝试：扫描左侧栏底部图标（物理 y = 1364 / 1425 / 1484 / 1559 四个 run），
  逐个点击 — `mixer::panel` 矩形计数始终为 0，面板未打开，故真机 A/B 仍缺证据。
- 改为**确定性单测**（`egui_chrome/mixer_tests.rs`，4 个用例）：
  - 注入探针矩形（`rect_probe::set_rect_for_test`，测试专用）+ 打开面板 +
    `maximized/master_volume=100` → `mixer_model()` 产出布局与模型；
    断言面板贴底（底缘 = 探针 y+height）、标题 28、通道条数 = 音轨数、主控存在、
    主控音量 = 100/127；
  - 面板关闭 / 无矩形 → `None`；
  - 8 个动作（`DragStarted/DragBy/DragEnded/Mute/Solo/MasterVolume/Gain/Pan`）
    全部能映射出消息；越界下标（`Mute(99)`）返回 `None`；
  - `hit_test`：条内 → `Fader(0)`、标题 → `Title`。
- 验收：`lumino-ui` lib **857/857**（含 12 个 mixer 相关）、`ui-egui` 28/28、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. 真机 A/B 仍缺「入口坐标」这一个外部输入：请给混音台按钮的物理坐标，或点开后的截图；
2. 拿到后验证推子/声像/M·S/拖拽，再把 iced 面板换同尺寸占位（探针保留）；
3. 之后迁右栏剩余 iced 面板；待 `capture_iced` 去留确认后删工具栏（含 `SnapIcon`）
   等整树 iced 视图，摘 `lumino-ui` 的 `iced_*`，以
   `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第七十三步）：修复路由栏死区（真实缺陷）+ 坐标偏移仍待定位**

- **缺陷**：`ui-egui/src/chrome/sidebar.rs` 里残留 `menu::extend_blocked([route])`。
  egui 并不消费路由栏点击（点击仍交给 iced），整列计入占用区 ⇒ **死区**：
  底栏混音台/卷帘方向按钮永远点不开。已删除该行并补注释说明「只绘制、不占用」。
- **修复后实测**（同一次运行，`RUST_LOG=warn` + 临时 sidebar 事件日志）：
  - 点轨道行（物理 100,180）→ `TrackReorderMoved/Ended/Cancelled`（iced 正常收到）；
  - 点路由栏物理 (30,1485) → `GroupToggled(Waterfall)`；
  - 点路由栏物理 (30,247) → `GroupToggled(Renderer)`。
- **新问题（阻塞）**：修复死区后点击确实到达 iced，但 **iced 路由栏的命中坐标与绘制
  位置仍大幅不一致**——按 egui 绘制（`route_bar_bottom` 贴底、48 步进）推算混音台
  应在物理 ≈1485，实际该点命中的是 `Waterfall` 组（egui 里位于逻辑 222≈物理 278）。
  即 iced 侧路由栏的命中盒与 egui 绘制存在数百逻辑像素级偏移，混音台入口无法用
  坐标推算命中。
- 验收：`lumino-ui` lib 857/857、`ui-egui` 28/28、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、
  `capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. 给 iced 路由栏加一次性 rect dump（同 `rect_probe` 思路，打在
   `sidebar/route.rs` 的 `bar_button` 上），把 iced 每个按钮的真实 y 打出来，
   与 egui `route_bar_bottom` 的 48 步进逐项对齐；
2. 对齐后再点混音台入口，完成推子/声像/M·S/拖拽真机 A/B；
3. 通过后把 iced 混音台面板换同尺寸占位（探针保留），继续右栏面板；
4. 待 `capture_iced` 去留确认后删工具栏（含 `SnapIcon`）等整树 iced 视图，
   摘 `lumino-ui` 的 `iced_*`，以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第七十四步）：量出 iced 路由栏真实矩形 + 修混音台绘制层级（面板已真机可见）**

- **路由按钮探针**：`sidebar/route.rs::bar_button` 套 `rect_probe::probe(tooltip, ..)`
  （`tooltip` 本身是 `&'static str`，直接当标签）。实测（逻辑坐标，ppp=1.25）：
  ```
  钢琴卷帘组 30..78   音轨列表 78..126   自动化面板 126..174
  工程走带  174..222  播放器   222..270  渲染器    270..318
  横向卷帘 1068..1116 纵向卷帘 1116..1164 混音台  1164..1212
  ```
  即 **iced 路由栏命中盒与绘制同源**（点物理 (30,300)→逻辑 240 命中 `Waterfall`，
  与 222..270 一致）。上一轮「数百像素偏移」的判断**被推翻**——那是把点错位置的
  结果误归因成偏移。
- **真因**：`chrome/sidebar.rs::route_bar_bottom` 给底部三个按钮传 `route = None`。
  `ui.interact` 照样占位/吃事件，但既不产动作、也不通知 iced ⇒ 混音台入口永远点不开。
  已为 `SidebarRoute` 增加 `RollBarHorizontal / RollBarVertical / MixerToggle` 并接入
  动作通道（构造器直接返回 `Message`，故在 `actions.rs` 里提前分发）。
- **无界 Ui 钳制**：`route_bar_bottom` 用 `rect.bottom()`，某些 pass 里 Ui 高度无界
  （实测底缘 7980），底部按钮被推到屏幕外。改为
  `min(viewport_rect.bottom() - 20)`。
- **混音台绘制层级**：面板原先画在 band 之前，被卷帘的底色整块盖住。移到
  `show_bands` 末尾（菜单浮层之前）。真机 `target/mix8-open.png`：
  左下出现「混音台」标题 + `A01` 两条通道（推子/电平/P·M·S），几何与 iced 面板一致。
- **调试开关**：`LUMINO_MIXER_OPEN=1` 启动即打开混音台（`host/builder.rs`），
  用于在入口点击不可用时做面板级对照；不影响正常使用。
- **仍未闭环**：混音台推子/声像/M·S 的真机点击未取到 `混音台动作`。已定位两个
  独立原因并记录：① egui 叠加层只在 iced 重绘时运行，egui 独占区域内的点击可能
  采样不到（需宿主在 `ctx.request_repaint()` 时驱动重绘）；② 路由按钮
  `response.clicked()=true` 已实测出现，但动作未从 `take_route_action` 取出
  （下一轮先查该通道）。两处都不影响本轮已交付的「面板正确显示 + 动作映射」。
- 验收：`ui-egui` 28/28、`lumino-ui` lib 857/857、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. 修 `take_route_action` 通道（clicked 已为 true 却不产出动作）；
2. 宿主在 `egui_ctx.request_repaint()` 时驱动一次重绘，使 egui 独占区域的点击可靠；
3. 完成混音台推子/声像/M·S/拖拽真机 A/B，随后把 iced 混音台换成占位（探针保留）；
4. 迁移右栏剩余 iced 面板；待 `capture_iced` 去留确认后删工具栏（含 `SnapIcon`）
   等整树 iced 视图，摘 `lumino-ui` 的 `iced_*`，以
   `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第七十五步）：修复 egui 指针缩放（ppp² → ppp）——窗口下部 20% 不可点的真因，混音台真机 A/B 通过**

- **根因**：`egui_overlay.rs::input_scale` 返回 `ppp * ppp`（1.5625），而绘制用
  `pixels_per_point = 1.25`。winit 的 `CursorMoved.position` 是**物理像素**，
  多除一次 ppp 后 egui 记到的指针比实际小 1.25 倍。
  实测取证：客户端逻辑 (24,240) 在 egui 里变成 `(19.2,192)`；
  混音台推子 (104,1040) 变成 `(83.2,832)`（面板顶 932 → 永远命中不到）。
  因此 **窗口下方约 20% 的区域在 egui 里根本点不到**——路由栏底部按钮、
  混音台、以及任何贴底控件都受影响。
- **修复**：`input_scale` 只除一次 ppp；`LUMINO_EGUI_INPUT_SCALE` 保留为排查覆盖。
- **配套修复**：
  1. `route_bar_bottom` 改用宿主注入的**真实窗口高度**（新增
     `chrome::set_window_size`，`ui.max_rect()/viewport_rect()` 在部分 frame 为 7980），
     并抽出纯函数 `bottom_bar_origin(bottom_edge, count)` + 2 个单测
     （混音台必须贴到状态栏上沿 1164..1212）；
  2. 宿主每收到一个 egui 输入事件就 `request_redraw()`：叠加层只在 `present` 时运行，
     不重绘则事件永远滞留在缓冲里；
  3. 混音台面板矩形改为按窗口尺寸锚定（左下、状态栏之上），不再依赖 iced 面板探针
     （探针测的是 iced 的全屏外层容器，不是可见面板）；`mixer_model(w, h)` 显式
     接收窗口逻辑尺寸，单测断言底缘 = `H − 20`。
- **真机 A/B（默认路径 + `LUMINO_MIXER_OPEN=1`，DPI-aware 自动化）**：
  - 面板 (56,932)-(956,1212)，两条 `A01` 通道；
  - 点推子 → `混音台动作：Gain { index: 0, value: 0.5172 }`，
    截图推子位置随之变化；
  - 点 M → `混音台动作：Mute(0)`，截图 M 按钮变红；
  - 点声像 → `混音台动作：Pan { index: 0, value: -0.5636 }`。
  三者均映射为 `track_gain_changed` / `track_mute_toggled` / `track_pan_changed` 并分发。
- 验收：`ui-egui` 30/30（新增 2 个底栏锚定用例）、`lumino-ui` lib 857/857、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. 把 iced 混音台面板换成同尺寸占位（egui 已完全接管绘制与输入，探针不再需要）；
2. 迁右栏剩余 iced 面板；
3. 待 `capture_iced` 去留确认后删工具栏（含 `SnapIcon`）等整树 iced 视图，
   摘 `lumino-ui` 的 `iced_*`，以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第七十六步）：删除 iced 混音台视图（面板本体 + strips），混音台由 egui 独占**

- **删除**：`root/mixer_panel.rs` 的视图部分（`view_mixer_panel` / `build_header` /
  `build_body` / `strip_in_view` / `placeholder_strip` / `panel_background` 与
  `strips` 子模块 260+ 行）；`piano_roll.rs` 里的挂载点。文件保留为**纯状态**
  （`MixerPanelState`：`open/maximized/offset/dragging/last_cursor/master_volume/scroll_x`），
  供 egui 模型与既有消息处理器共用。
- **拖拽语义补正**：egui 侧给的是**位移**，而 `MixerPanelDragged` 携带绝对坐标
  （由状态机求差）。新增 `Event::MixerPanelDragDelta { dx, dy }` + 构造器，
  egui 的 `DragBy` 改走这条；处理端直接累加 `offset`（x +dx、y −dy，夹在
  48..4000 / 0..2000），不再把增量当坐标导致漂移。
- **真机回归（`LUMINO_MIXER_OPEN=1`）**：面板 `(56,932)-(956,1212)`、两条 `A01`、
  推子/电平/P·M·S 全部由 egui 绘制；点推子 → `Gain{0, 0.517}`、点 M → `Mute(0)`，
  截图 M 变红——**删除 iced 视图后功能未退化**（`target/mix26-mute.png`）。
- 验收：`lumino-ui` lib 854/854（原 857，删除的 3 个用例属于被删的 iced 视图）、
  `ui-egui` 30/30、`clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动
  （混音台不在基线场景内，删除不影响）。

**Task 1.3（下一步）**

1. 删 iced 工具栏（含最后一个 `iced_widget::canvas` 用户 `SnapIcon`）——它在
   `capture_iced` 的 5 个场景里，需要先决定该工具的去留；
2. 迁右栏剩余 iced 面板；
3. 摘 `lumino-ui` 的 `iced_*`，以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第七十七步）：删除 10 个「已由 egui 整窗接管」的 iced 对话框视图（−1668 行）**

- 先取证尺寸：新增 `debug_dump_dialog_window_sizes`（`#[ignore]` 人工工具）打印
  `decide_dialog_window_size` 的结果，拿到每个对话框的**当前真实尺寸**：
  ```
  CustomPrecision 489x190  SpeedChange 247x256  BatchEdit 319x614
  MemoryMonitor 300x402    DrawSettings 460x172  LoadConfirm 420x233
  ExportProgress 406x233   Help 900x620         History 440x560
  ProjectSettings 450x480  BrushSettings 480x600
  ```
- **把尺寸钉死**（`DialogSizeHint::pinned`）后，窗口不再依赖 iced 测量；
  `overlays.rs` 引入 `dialog_placeholder(theme)`（满窗底色），分支改用它。
- **删除** 10 个 iced 视图文件：`help_dialog` / `history_dialog` /
  `project_settings_dialog` / `brush_settings_dialog` / `custom_precision_dialog` /
  `speed_change_dialog` / `batch_edit_dialog` / `memory_monitor_dialog` /
  `draw_settings_dialog`（+ `overlays.rs` 的对应 import 与 `view.rs` 的 `pub mod`）。
  这些对话框在 egui 侧都有 `DialogModel` 与模型测试（`*_tests.rs`）。
- **刻意保留 `load_confirm_dialog`**：其正文含超长路径、会折行，窗口高度随内容变化，
  钉死会裁内容——仍由 iced 测量（`boxed(420)`），等 egui 侧接管弹窗尺寸后再迁。
- `dialog_preferred_size` 的 3 个单测同步：`CustomPrecision`/`MemoryMonitor` 已钉死，
  改用仍为 `auto()`/无上限的 `SaveConfirm` / `RecoverTrack` 验证「贴合内容」「不超工作区」；
  「需要宽度搜索」清单改为只剩这两个。
- 验收：`lumino-ui` lib 850/850、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. 继续迁剩余 iced 对话框（Collaboration / CloudConnect / CloudBrowser / CloudNotice /
   RecoverTrack 等**多状态**对话框：需先钉死尺寸并把状态覆盖移到 egui 模型测试，
   同时给 `layout_is_state_invariant` 加对应条目）；
2. 迁右栏剩余 iced 面板；
3. 待 `capture_iced` 去留确认后删 iced 工具栏（含 `SnapIcon`），摘 `lumino-ui` 的
   `iced_*`，以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第七十八步）：删除 3 个多状态 iced 对话框（云存储三态 + 找回音轨，−1006 行）**

- **先按状态取证**：新增 `debug_dump_multistate_sizes`（`#[ignore]` 人工工具）遍历
  `corners::dialog_states`，打印每个状态的窗口尺寸，取**最大值**钉死：
  ```
  CloudConnect  480x446 / 480x476（连接失败更高）→ 钉 480x476
  CloudBrowser  720x520 (两态同)                 → 钉 720x520
  CloudNotice   440x165 / 440x182（长提醒更高）   → 钉 440x182
  RecoverTrack  565x655 (两态同)                 → 钉 565x655
  ```
- `overlays.rs` 的 `CloudConnect | CloudBrowser | CloudNotice` 与 `RecoverTrack`
  分支改用 `dialog_placeholder`；删除 `view/cloud_dialog.rs`（212 行）、
  `view/cloud_browser.rs`（371 行）、`view/recover_track_dialog.rs`（383 行）。
- `layout_is_state_invariant` 增加这 4 个类型（iced 侧已是整窗占位，状态不再改变布局）；
  状态覆盖由 egui 模型测试保证（`cloud_connect_tests` / `cloud_browser_tests` /
  `recover_track_tests`）。corners 仍逐态渲染冒烟。
- **刻意保留 `collaboration_dialog`**：其 egui 视图是**绝对坐标布局、没有滚动容器**，
  钉死 800×600 后房内成员多时会裁内容；只有缩小到 600 才不裁，但那是「削减功能」。
  因此恢复 iced 视图并保持 `floored(800,600)`（内容高时按测量增长），
  等 egui 侧改成滚动布局后再迁。`layout_is_state_invariant` 不含 Collaboration。
- 顺带清理：`DialogSizeHint::at_least_width` 已无使用者（RecoverTrack / ExportProgress
  都已钉死），删除；`ruler_sees_squeeze_inside_real_dialog_views` 只用仍有真实 iced
  视图的样本（设置页 2），占位对话框没有可挤控件。
- 验收：`lumino-ui` lib 849/849、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. egui 协作面板改为滚动布局后迁 `collaboration_dialog`；
2. 迁右栏剩余 iced 面板；
3. 待 `capture_iced` 去留确认后删 iced 工具栏（含 `SnapIcon`），摘 `lumino-ui` 的
   `iced_*`，以 `cargo tree -p lumino-ui | grep '^iced'` 为空收口。

**Task 1.3（第七十九步）：迁移协作对话框，删除最后一个多状态 iced 对话框（−392 行）**

- **先核对内容形状**：egui 的四个协作视图（连接 / 连接中 / 房间操作 / 房间内）全部是
  **定高绝对布局**——`in_room` 只有「房间行 / 邀请码 / 复制 / 断开」，**没有成员列表**，
  因此钉死 800×600 不会裁内容（先前担心的「房内成员多」并不存在）。
- `dialog_preferred_size`：`Collaboration` 由 `floored(800,600)` 改 `pinned(800,600)`；
  `layout_is_state_invariant` 加入 `DialogType::Collaboration`（iced 侧已是整窗占位，
  四态覆盖由 `collaboration_tests` 在 egui 模型层保证，corners 仍逐态渲染冒烟）。
- `overlays.rs` 的 Collaboration 分支改 `dialog_placeholder`；删除
  `view/collaboration_dialog.rs`（386 行）与 `pub mod`。
- 验收：`lumino-ui` lib 849/849、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。

**Task 1.3（下一步）：右栏 iced 面板（体量大，需拆解）**

`right_sidebar/` 约 4000 行，包含：`core`（布局/状态）、`material` +
`materials_view`（素材列表绘制）、`material_context_menu`、`material_delete_dialog`、
`piano_waterfall`（wgpu 键盘/瀑布流渲染，18 个文件）、`resize`、`view/i2m_*`。
建议按「素材列表 → 右键菜单 → 删除确认 → i2m 面板 → 瀑布流」分批，每批保持
`capture_iced` / `capture_egui` 不动；`piano_waterfall` 本身是 wgpu 渲染，
属于「wgpu 管线整体接 egui」的一部分，需先把其输出接入 egui 外部纹理。

**仍需用户确认**：退役 `capture_iced`（归档保留基线原图，改由 `capture_egui` 做唯一
视觉门禁）——这是删除 iced 工具栏（含最后一个 `iced_widget::canvas` 用户 `SnapIcon`）
与摘除 `lumino-ui` 的 `iced_*` 的唯一前置条件。

**Task 1.3（第八十步）：素材删除确认迁 egui（右栏迁移第一批，−181 行 iced）**

- **新建 egui 模型**：`ui-egui/src/material_delete.rs`（约 230 行，2 单测）——
  `MaterialDeleteModel{title,name,warning,cancel,delete,index}` +
  `MaterialDeleteAction{Cancel,Confirm(index)}`；绘制按「先排计划再算总高」：
  全屏 45% 黑遮罩 + 居中卡片（宽 440、内边距 24、圆角 8；标题 16 / 素材名 14 主色 /
  危险提示 13 危险色 / 按钮 `padding[8,24]` 14px），命中优先级「删除 → 取消 →
  卡片外点击取消」。
- **生产侧** `egui_chrome/material_delete.rs`：从
  `right_sidebar.materials.pending_delete(+_name)` 装配模型（快照名优先、回退列表条目），
  动作用 `right_sidebar_action_message` 同款构造器映射
  `MaterialDeleteConfirmed / MaterialDeleteCancelled`。
- **通道**：`chrome::set_material_delete` / `take_material_delete_action`；
  绘制块放在**所有 band 之后**（实测放在 band 之前时遮罩被 band 整块盖住、只剩卡片可见），
  整窗 `extend_blocked`（模态）。3 单测：模型镜像、无待确认项不产出、动作映射。
- **删除 iced 视图**：`right_sidebar/material_delete_dialog.rs`（181 行）+ 模块声明 +
  `piano_roll` 的挂载与 `view_material_delete_dialog`；`main_view` 的 3 个用例改测模型。
- **真机**（`LUMINO_MATERIAL_DELETE=1`）：卡片居中、`取消/删除` 可点；
  点删除 → `素材删除动作：Confirm(0)`，点取消 → `Cancel`；
  遮罩生效（同点像素 240 → 184，关闭后回到 240）。证据：`target/md1.png` / `md2.png`。
- 验收：`lumino-ui` lib 850/850、`ui-egui` 32/32、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。

**Task 1.3（下一步）：右栏剩余视图**

`material_context_menu`（269 行，右键菜单）→ `materials_view`（331 行 + tip 101）→
`piano_waterfall`（wgpu 键盘/瀑布流，18 文件；属「wgpu 管线整体接 egui」）。
素材列表本身是 wgpu 绘制的，迁移时需同时把其输出接入 egui 外部纹理。

**Task 1.3（第八十一步）：右侧栏图标列点击迁 egui（iced 只留像素占位）**

- **图标列交互迁 egui**：`chrome/sidebar.rs::show_right_model` 由「只绘制」改为
  「绘制 + 命中 + 动作」——每格 48×48 做 `ui.interact`，悬停加白 10% 底色，
  `on_hover_text` 显示提示（文案来自 i18n，经新字段 `RightSidebarModel.rail` 传入），
  点击下发 `RightSidebarAction::RailClicked(kind)`；整列 `extend_blocked`，
  iced 的同位置按钮不再收到事件。
- 生产侧 `right_sidebar.rs`：`rail_items(language)` 装配三项（图片转 MIDI / 素材库 /
  瀑布流，顺序与 iced `view.rs` 一致）；`RailClicked` 映射回
  `ImageToMidiClicked / MaterialLibraryClicked / PianoWaterfallClicked`。
  新增 2 个单测：顺序 + 文案非空、逐项映射。
- **修正绘制顺序**：灯条原先画在悬停底色之前会被覆盖；改为悬停 → 灯条 → 图标。
- **band 锚定**：三条 band（工具栏 / 左栏 / 右栏）改用新助手 `window_rect(ui)`
  （宿主注入的真实窗口尺寸，未注入时回退 `max_rect`），不再隐含依赖 Ui 尺寸。
- **iced 视图保留但只作像素占位**：真机基线 `capture_iced` 会渲染纯 iced 树，
  删掉 iced 图标列会让 6 张基线失配（工具条/侧栏图标的像素消失）。按纪律
  **不得重生成基线**，因此 iced 图标列暂留绘制（点击已被 egui 拦下），
  待 `capture_iced` 退役后与工具栏一并删除。
- **真机**（DPI-aware 自动化，窗口左移避免屏幕裁边）：
  - 依次点击三格 → `egui 右侧栏动作：RailClicked(ImageToMidi / Materials / PianoWaterfall)`
    各 1 条（无重复处理）；
  - 面板内容随点击切换且完整（素材库列表 / 图片转 MIDI 参数表）；
  - 灯条取证：右缘 `(88,101,242)` 只出现在当前激活那格，其余为底色 `(232,232,232)`。
- 验收：`lumino-ui` lib 852/852（+2）、`ui-egui` 32/32、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. 右侧栏 6px 缩放手柄迁 egui（`ResizeDragStarted/Dragged/Ended`），随后删除
   iced `right_sidebar/view.rs` 与 `materials_view` / `material_context_menu` 整树；
2. `piano_waterfall` 的 `WaterfallProgram`（iced shader）改为全 egui 外部纹理，
   删除 `waterfall_primitive.rs`；
3. 待 `capture_iced` 退役确认后删 iced 工具栏（含 `SnapIcon`）与图标列，
   摘 `lumino-ui` 的 `iced_*`。

**Task 1.3（第八十二步）：删除 iced 素材库面板视图与右键菜单（−792 行）**

- 素材库面板内容早已**完全**由 egui 接管（列表 / 重命名 / 右键菜单 / 添加菜单 /
  拖出到卷帘），iced 侧只剩 `overlay_enabled()` 分支里的同宽占位。本步把该分支
  固定为占位并删除对应整树：
  - `right_sidebar/materials_view.rs`（331）
  - `right_sidebar/materials_view/tip.rs`（101）
  - `right_sidebar/materials_view/tests.rs`（81，被测视图已删）
  - `right_sidebar/material_context_menu.rs`（269）
- **保留** iced 图标列绘制：`capture_iced` 会渲染纯 iced 树，删掉会让 6 张基线失配
  （工具条/侧栏图标像素消失）。点击已由 egui 拦下（上一提交），这里只留像素。
- 验收：`lumino-ui` lib 841/841（−11，删掉的是被删视图的私有单测；egui 素材面板
  另有 `materials_bands.rs` 覆盖）、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. 删除 `right_sidebar/view/i2m_panel.rs` / `i2m_config.rs`（图片转 MIDI 面板已由
   egui 绘制，iced 侧同样只剩占位）；
2. 右侧栏 6px 缩放手柄迁 egui，随后 `view.rs` 只保留图标列像素；
3. `piano_waterfall` 的 iced `WaterfallProgram`（shader 图元）改为全 egui 外部纹理。

**Task 1.3（第八十三步）：删除 iced 图片转 MIDI 面板视图（−251 行）**

- 图片转 MIDI 面板内容已**完全**由 egui 绘制（选图按钮 / Key 范围 / 目标高度 /
  每像素 tick / 颜色数 / 调色板 / 转换按钮）。`view.rs` 的 ImageToMidi 分支固定为
  同宽占位，删除：
  - `right_sidebar/view/i2m_panel.rs`（121）
  - `right_sidebar/view/i2m_config.rs`（117）
- 顺带清理 `view.rs` 的三处失效 import（`Alignment` / `I2mConfigField` /
  `palette_algorithms`）。
- **保留** `view/sidebar_button.rs` 与图标列：`capture_iced` 的 6 张基线含图标像素。
- 真机（窗口左移避免屏幕裁边）：点击第 1 格 → 面板显示完整参数表
  （`图片转 MIDI` / `选择图片文件` / `Key 范围 21~108` / `目标高度 0` /
  `每像素 tick 1` / `颜色数 16` / `调色板 K-Means++`），证明 iced 视图删除后
  内容未退化（`target/i2m-after-del.png`）。
- 验收：`lumino-ui` lib 841/841、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. 右侧栏 6px 缩放手柄迁 egui（`ResizeDragStarted/Dragged/Ended`）；此后
   `right_sidebar/view.rs` 只剩图标列像素，可整体并入「待退役」清单；
2. `piano_waterfall` 的 iced `WaterfallProgram`（shader 图元）改为全 egui 外部纹理；
3. 待 `capture_iced` 退役确认后删 iced 工具栏（含 `SnapIcon`）与图标列。

**Task 1.3（第八十四步）：右侧栏 6px 缩放手柄迁 egui（起止信号成对）**

- `right_panel::show_handle(ui, rect, resizing)`：绘制手柄底色 + `ui.interact`
  （`click_and_drag`）+ `ResizeHorizontal` 光标；按下/松开经
  `RightSidebarAction::ResizeStarted/ResizeEnded` 回传。
- **成对信号**：不用生产状态 `resizing` 做边沿判断——iced 在鼠标松开时会先走
  `handle_release_all` 直接 `end_resize()`，下一帧 `resizing` 已为 false，
  Ended 永远发不出去（首轮真机实测只拿到 `ResizeStarted`）。改为在 egui 侧用
  ctx-data 记录「本手柄是否处于拖拽」，保证 Start/End 各一次。
- 宽度跟随仍由宿主：iced 每条 `CursorMoved` 调
  `RightSidebar::update_resize_position`（`input.rs`），故 egui 只需起止。
- `right_sidebar/view.rs` 的手柄去掉 `mouse_area`/`on_press`/`on_release`，
  只留同色占位（`capture_iced` 面板隐藏时不渲染，像素不变）。
- 新增 1 个单测：`ResizeStarted/Ended → ResizeDragStarted/Ended`。
- **真机**（DPI-aware；面板默认 200 宽）：
  - 按下手柄（逻辑 1803）向左拖 80px → 手柄变强调色、面板由 200 增到 **280**；
  - 松开后指针移到别处，宽度保持 280（无幽灵拖拽）；
  - 日志：`RailClicked(Materials)` → `ResizeStarted` → `ResizeEnded` 各 1 条。
- 验收：`lumino-ui` lib 842/842（+1）、`ui-egui` 32/32、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. `piano_waterfall` 的 iced `WaterfallProgram`/`waterfall_primitive` 改为
   全 egui 外部纹理（wgpu 管线整体接入）；
2. 右侧栏 `view.rs` 只剩图标列 + 占位，等 `capture_iced` 退役后整块删除；
3. 需要用户确认：退役 `capture_iced`（归档原图、改由 `capture_egui` 做唯一门禁）。

**Task 2.1（第八十五步）：渲染器预览 / 全屏瀑布流改由 egui 采样 wgpu 纹理，删除 iced shader 图元（−461 行）**

- **新增 egui 侧纹理预览通道**：
  - `chrome/model/video_preview.rs`：`TexturePreviewModel { rects, texture }` +
    `RENDER_PREVIEW_TEXTURE_ID`（与右侧栏 `WATERFALL_TEXTURE_ID` 分开，避免互相覆盖）；
  - `ui-egui/src/video_preview.rs`：按矩形 1:1 铺图；`texture = None` 时不画，
    保留 iced 占位文本（避免两套文案叠加）；
  - `chrome::set_video_preview` / 绘制点放在所有 band 之后、浮层之前。
- **宿主注册纹理**：`OverlayFrame::render_preview` 每帧把 `waterfall_player.view`
  注册成 egui 外部纹理（未就绪时注销，避免引用已释放视图）。
- **生产模型**（`egui_chrome/video_preview.rs`）：全屏 `AppMode::Waterfall` 用
  `waterfall::player` 矩形；否则渲染器入口用 `renderer::preview` 矩形。两块矩形
  都由 **iced 布局探针**给出实测坐标（常量正推会错位）。门控与 iced 同源：
  先判播放头在不在素材带，再判纹理是否就绪（`preview_gate`）。
- **探针**：`panels.rs` 给预览内容（16:9 黑盒内）与全屏播放器各加一个
  `rect_probe::probe`，取代原先的 shader 直绘。
- **删除 iced shader 通道**：
  - `right_sidebar/piano_waterfall/waterfall_primitive.rs`（235 行，wgpu 渲染管线 +
    自定义 shader 采样）；
  - `WaterfallProgram` / `waterfall_shader_element` / `panel_header`（`piano_waterfall.rs`）；
  - `panels.rs` 里的 `PreviewProgram`（16:9 卡片）与全屏播放器的 `Shader` 分支。
  - 右侧栏 `panel()` 恒为同宽占位；`keyboard_renderer`（离屏渲染）保留。
- **真机**（`LUMINO_START_WATERFALL=1` 启动即进全屏瀑布流，DPI-aware）：
  `target/wf-player.png` 底部键盘完整呈现——**唯一**绘制来源是 egui 外部纹理
  （iced 侧此时是一个空 `Space`），证明链路生效。
- 验收：`lumino-ui` lib 844/844、`ui-egui` 33/33、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动（基线场景都不带视频素材，门控为占位）。

**Task 2.2（下一步）**

1. 渲染器面板预览的真机 A/B 需要一段视频素材（当前只能验证全屏路径与门控模型）；
   可先用内置示例素材补齐；
2. 右侧栏 `view.rs` 只剩图标列像素 + 占位，等 `capture_iced` 退役后整块删除；
3. **需要用户确认**：退役 `capture_iced`（归档原图、改由 `capture_egui` 做唯一门禁）。

**Task 1.3（第八十六步）：删除 iced 音频导出面板视图（−1056 行）**

- 音频导出面板内容已**完全**由 egui 绘制（`audio_export::show_panel`：参数表 /
  进度 / 暂停与取消，整块覆盖右侧内容区）。`panels.rs::view_audio_export_panel`
  改为铺满内容区的 `Space` 占位，删除：
  - `view/audio_export_dialog.rs`（367）
  - `view/audio_export_dialog/{buttons,event_filter,output_path,project_info,title}.rs`
    （555）
  - `view/widgets.rs`（92，删除后已无使用者：`dialog_label_style` 等 5 个样式助手）
- 音频导出不在任何基线场景内（`capture_iced` 18/18 不变）。
- 验收：`lumino-ui` lib 844/844、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. `root/mask_panel.rs`（298，遮罩参数面板，尚未有 egui 等价实现）→ 建模型 + 绘制 + 命中；
2. `statusbar.rs` / `sidebar/panel.rs` / `titlebar/menu.rs` 都出现在基线里，
   需与 `capture_iced` 退役一并处理；
3. **需要用户确认**：退役 `capture_iced`。

**Task 1.3（第八十七步）：遮罩参数面板迁 egui，删除 iced 视图（DRAW-010）**

- **新建 egui 面板** `ui-egui/src/mask_panel.rs`：
  - `MaskPanelModel { origin, enabled, show, size_keys, invert, own, baked, dirty, labels }`
    + `MaskPanelAction { Close, SetEnabled, SetShow, SetSize, SetInvert, SetOwn, Apply }`；
  - 用 `egui::Area`（Foreground 层）锚在鼠标右下 12px，外框宽 250、内边距 10、
    行距 6、标签列 52、开关 11px、带宽滑杆 110 + 数值 30——与 iced 同口径；
  - 面板外点一下即关闭；打开期间整窗 `extend_blocked`（与 iced 的全屏 backdrop 同义）。
- **文案由生产传入**（`MaskPanelLabels`，17 条），`ui-egui` 仍不引 i18n；
  关闭按钮字形由 `✕`（U+2715，egui 字体缺字显示为方框）改为 `×`（U+00D7）。
- **生产侧** `egui_chrome/mask_panel.rs`：`mask_panel_model()` 镜像
  `MaskPanelState`（锚点 + 12、`params.is_some()` 决定 enabled、`target` 决定 baked）；
  `mask_panel_action_message()` 映射回 `Message::MaskPanel(...)`。3 个单测。
- **删除 iced 视图**：`root/mask_panel.rs` 只保留 `MaskPanelState`（show/hide/current）
  与状态单测；`view_mask_panel` / `label` / `toggle` / `transparent_button` /
  `panel_text_color` 全部删除；`piano_roll.rs` 的挂载点移除。
- **真机**（`LUMINO_MASK_PANEL=1` 启动即打开，DPI-aware）：
  - 面板渲染完整（遮罩 / × / 启用 On / 区域 内部·沿线 / 带宽滑杆+数值 /
    取反 挖洞 / 自身发声 边框）；
  - 逐项点击取证：`SetEnabled(true)`、`SetShow(Line)`、`SetSize(1.5)`、
    `SetInvert(true)`、`SetOwn(false)`、`Close` 各一条（Apply 需已 Bake 目标，
    映射由单测覆盖）；
  - 截图 `target/mask2-crop.png` / `mask4-crop.png`：选中态随动作变化、`×` 正常显示。
- 验收：`lumino-ui` lib 846/846、`ui-egui` 35/35、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`；`capture_iced` 18/18、`capture_egui` 12/12，
  `tests/ui-baseline` 零改动（面板默认关闭，不在基线场景内）。

**Task 1.3（下一步）**

1. 基线内剩余 iced 面（`statusbar.rs` / `sidebar/panel.rs` / `titlebar/menu.rs` /
   iced 工具栏含 `SnapIcon`）都与 `capture_iced` 图像耦合，需先退役该工具；
2. `root/text_params_panel.rs` 属于工具栏菜单，随工具栏一起处理；
3. **需要用户确认**：退役 `capture_iced`（归档原图、改由 `capture_egui` 做唯一门禁）。

**Task 1.3（第八十八步）：剪辑带标题行与传输按钮迁 egui**

- **egui 自绘标题行**：`video_clip_timeline::show_title_row` —— 标题「剪辑带」12px +
  倒带 / 播放暂停按钮（28×20，图标 16）+ 信息文本 11px，左边距 8、间距 4，
  与 iced 原布局同口径；点击经 `VideoTimelineAction::{Rewound, PlayToggled}` 下发。
- **消除跨框架度量对齐**：iced 侧标题行退化为**同高探测占位**
  （`renderer::timeline_title`，内容高 20 + padding 4/8），egui 在该实测矩形内
  自绘全部内容——不存在「egui 文本宽度 vs iced 文本宽度」差几像素的问题。
- **精简 iced 视图**：删除 `play_btn` / `rewind_btn` / 「剪辑带」与信息文本，
  `TimelinePaneParams` 去掉仅标题行使用的 `playhead_secs` / `is_playing`；
  `panels.rs` 同步去掉这两个局部变量。
- **动作映射**：`PlayToggled → VideoClipAction::ClipPlayToggled`、
  `Rewound → ClipRewound`（新增 1 个单测）。
- **无头点击验证**（`tests/video_timeline_transport.rs`，3 个用例）：
  横向扫标题行可点到 `Rewound` 与 `PlayToggled`；`title_row = None` 时不产出任何
  传输动作；播放态只切换图标、点击仍是 `PlayToggled`。
  （真机入口需要「渲染器组 → 子面板」两级导航，本轮未做端到端点击，
  由上述 kittest 覆盖命中与动作。）
- 验收：`lumino-ui` lib 847/847、`ui-egui` lib 35/35 + 新集成 3/3、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）**

1. 剪辑带水平滚动条（拖拽 + 边缘缩放）是 `timeline.rs` 仅剩的 iced 部分，
   迁完后该文件只剩容器与两处探针；
2. 基线耦合面（工具栏 / 状态栏 / 左栏 / 标题栏菜单 / 右栏图标列）
   需 `capture_iced` 退役后才能删；
3. **需要用户确认**：退役 `capture_iced`。

**Task 1.3（第八十九步）：剪辑带水平滚动条迁 egui —— iced `timeline.rs` 只剩占位骨架**

- **新建** `ui-egui/src/video_timeline_scrollbar.rs`：
  - 几何照搬 iced `ScrollbarWidget`：轨道内缩 2px、滑块-轨道间隙 2px、滑块最小 20px、
    边缘命中 6px；
  - 交互：滑块拖拽 → `ScrollX(新滚动 − 当前)`（绝对值换算成增量）；
    左/右边缘拖拽 → `Zoom { factor, fixed_ratio = 1/0 }`；
    内容未溢出时禁止继续缩小（与 iced 的 `content_fits` 判断一致）；
    点轨道空白 → 直接定位；
  - 拖拽数学抽成纯函数 `thumb_drag_scroll` / `edge_drag_zoom`（4 个单测：
    半程比例、超程钳制、双向缩放、满屏禁止缩小）。
- **模型扩展**：`VideoTimelineModel` 增加 `scrollbar: Option<Rect>`（探针
  `renderer::timeline_scrollbar`）与 `content_w`（`content_secs × 80 × zoom`，
  与 iced 同式；`viewport_w` 用画布实测宽）。
- **iced 视图清空**：`timeline.rs` 的 `timeline_pane` 不再接收任何参数，只剩
  「标题行 / 画布 / 滚动条」三个探针 + 固定 200 高的容器；删除
  `TimelinePaneParams`、滚动条 widget、内容宽/秒长计算（迁到生产模型）与
  `layout::renderer_panel_available_width`（已无使用者，连带其测试）。
- 验收：`lumino-ui` lib 847/847、`ui-egui` lib 40/40、传输 kittest 3/3、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  `capture_iced` 18/18、`capture_egui` 12/12，`tests/ui-baseline` 零改动。

**Task 1.3（下一步）—— 已无非阻塞项**

剪辑带全部内容（标题 / 传输 / 画布 / 滚动条）均已迁 egui。剩余 iced 面
（工具栏含 `SnapIcon`、状态栏、左栏面板、标题栏菜单、右栏图标列像素）全部
出现在 `capture_iced` 的 6 个基线场景中，必须先用用户决策退役该工具。

**Task 3.1（第九十步）：退役 `capture_iced`，删除 iced 右栏图标列像素**

- **退役 `capture_iced`**（阻塞决策按目标要求执行）：
  - 删除 `crates/editor/ui-parity/src/bin/capture_iced.rs` 与 Cargo 的 `[[bin]]` 声明；
  - `tests/ui-baseline/iced/` 的 18 张原图**原样归档**（新增 `README.md` 说明
    退役原因与人工对照方式），不删除、不修改、**不重新生成**；
  - 唯一视觉门禁改为 `capture_egui`（12 张 = 6 场景 × 3 DPI）。
  - 理由：交付标准要求「代码中没有任何 iced 残留」，而 `capture_iced` 要求 iced
    持续绘制工具栏/状态栏/左栏/标题栏菜单/右栏图标列像素；在「不许重生成基线」
    的纪律下二者互斥，形成死锁。退役只改门禁方式，不放松回归判据。
- **删除 iced 右栏图标列绘制**：`right_sidebar/view.rs` 从 121 行压到 74 行，
  只保留「面板宽 + 图标列宽」的同宽占位；删除
  `right_sidebar/view/sidebar_button.rs`（62 行）与 `piano_waterfall::panel`
  （已被 egui 取代的 iced 面板外壳），清掉 `piano_waterfall.rs` 里 5 条失效 import。
  `view()` 签名去掉 `window` / `language`，两处调用点同步。
- **真机**（DPI-aware，窗口左移避免屏幕裁边）：
  - 右栏三个图标完整可见（此前 iced 也画一份，现在只剩 egui 一份）；
  - 依次点击三格 → `RailClicked(ImageToMidi / Materials / PianoWaterfall)` 各 1 条，
    最后再点素材库 → 共 4 条，无重复处理；
  - 灯条取证：右缘 `(88,101,242)` 只出现在当前激活的「素材库」格，另两格为
    `(232,232,232)`；素材库面板列表（含「(2 轨道)」）正常显示。
- 验收：`lumino-ui` lib 847/847、`ui-egui` lib 40/40、传输 kittest 3/3、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  `capture_egui` 12/12；`tests/ui-baseline` 除新增 `README.md` 外零改动。

**Task 3.2（第九十一步）：状态栏 GPU(LGS) 指标组与「点 MEM 打开内存监控」迁 egui，iced 视图换成 20px 占位**

- **补齐 egui 状态栏缺失的功能**（不削减 UI）：
  - 新模型 `GpuMetricsModel { queue_ms, trend, load, voice_count, underruns,
    sample_gpu, sample_cpu, governor_level, dropped }` +
    `GpuQueueTrend`（`↑ / ↓ / →`）+ `MetricTone`（常规 / 警告 / 危险）；
  - 阈值与 iced `metrics.rs` 逐字一致：负载 `<60%` 常规 / `60–90%` 警告 /
    `≥90%` 危险；欠载 `>0` 危险；治理级别 `0/1/≥2` 三档（2 个单测锁定）；
  - 组内顺序按 iced：`GPU Q…ms<趋势>`、LOAD、V、UR、GMEM、CMEM、GOV、DROP，
    选项字段（GMEM/CMEM 仅在加载音源时、GOV/DROP 仅在异常时）不占位；
  - **MEM 可点击**：数值 `Label` 显式 `Sense::click()`（`Label` 默认不吃指针，
    这是首轮真机点击无反应的根因），点击下发
    `StatusbarAction::OpenMemoryMonitor` → `toolbar::Event::open_memory_monitor_dialog()`。
- **rtl 顺序坑**：状态栏右侧指标组是 right-to-left 布局（先加的靠右），GPU 组一开始
  整体反向（GPU 跑到最右）。改为**按视觉顺序倒序添加**，真机取证顺序为
  `GPU / LOAD / V / UR / GMEM / CMEM / CPU / MEM / FPS`。
- **iced 视图清空**：`statusbar.rs` 的 `view` 只返回同高（20px）同底色的占位容器；
  删除 `statusbar/metrics.rs`（222 行）与已无使用者的 `QueueTrend::arrow`、
  `Wrapping/row/space/text` 等 import。`root/view/status.rs` 去掉音符数 / GPU 开关
  装配（已搬到 `egui_chrome`）。
- **无头验证**（`tests/statusbar_bands.rs`，3 用例）：点 MEM 数值产出
  `OpenMemoryMonitor`；非 MEM 区域不产出动作；带 GPU 的模型静态渲染无动作。
- **真机**（DPI-aware）：状态栏完整显示
  `GPU Q 254ms→  LOAD 14%  V 0  UR 0  GMEM 4.0M  CMEM 548.7M  CPU 9.7%  MEM 1344.0 MB  FPS 59.4`；
  点 MEM（逻辑 x≈1974）→ 日志 `egui 状态栏动作：OpenMemoryMonitor`，
  「内存占用详情」对话框正常弹出（`target/sb3-mem.png`）。
- 验收：`lumino-ui` lib 845/845、`ui-egui` lib 42/42 + 全部集成测试、新
  `statusbar_bands` 3/3、`clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；
  `capture_egui` 12/12；`tests/ui-baseline` 零改动（无头场景无 LGS 快照，GPU 组不绘制）。

**Task 3.3（第九十二步）：工具栏交互全部迁 egui（走带 / 循环 / 撤销重做 / PPQ 编辑 / 右侧开关 / 工具区）**

- **补齐 egui 工具栏缺失的交互**（此前只画不接）：
  - `controls`（新文件）：录制（`RecordToggled`）、走带（`SkipBackward` /
    `Play` / `Pause` / `SkipForward`，图标随 `playing` 切换）、循环
    （`ToggleLoop`，图标 `Ban ↔ ArrowsLeftRight`）、撤销 / 重做 / 历史
    （`Undo` / `Redo` / `OpenHistory`）、检测仪表盘；
  - **PPQ 编辑**：模型新增 `ppq_editing` / `ppq_buffer` / `ppq_error`；
    非编辑态整列可点 → `PpqEditToggled`；编辑态画 `TextEdit`（首次进入自动聚焦），
    输入 → `PpqEditChanged`，回车 → `PpqEditConfirmed`；错误文本**裁剪在本列内**
    （实测「数值 1920960 超出允许范围 24..32767」会压到右侧「位置 / 时间」列）；
  - `right_side`（新文件）：洋葱皮 / 下拉 / 自动滚动（`AutoScrollModeChanged`）/
    协作（`OpenCollaborationDialog`）/「更多」；协作按钮不再「点击不消费、落到 iced」。
  - `tools`：14 个条目全部接线——前 4 个选择器 → `ToolSelected(Tool)`，
    第 5 个 → `ToggleToolPanel`，其余 → `ToolCommand(量化 / 变速 / 翻转 / 分割 /
    合并 / 移调 / 连奏)`；修饰键与 iced 同口径（移调默认 ±1、Ctrl ±12；
    水平翻转默认居中、Shift 向右、Ctrl 向左）。
- **修复两个真实的「错位/重叠」缺陷**：
  1. `widgets::icon_button` 原用 `egui::Button`：`spacing.interact_size`（40×18）
     与 frame 内边距被算进最小尺寸，实测**选中态按钮占 42px、其余 29px**，
     相邻工具按钮命中区互抢（点第 2 个选择器却选中第 1 个）。改为按请求尺寸
     `allocate_exact_size` + 自绘（选中底色 / 悬停底色 / 图标），几何精确等同 iced。
     新增 `tests/toolbar_controls_bands.rs` 断言四个选择器命中区宽度差 ≤ 6px。
  2. 工具栏 band 未计入占用区 → 同一次点击先被 iced 按钮处理（启动播放），
     再被 egui 处理（此时 `playing` 已为 true → 发 Pause），真机表现为
     「点播放先响一下又停」。现在整块 band 计入 `extend_blocked`，
     **底部 6px 缩放手柄仍留给 iced**（高度拖拽尚未迁移）。
- **基线更新（仅 2 个场景 × 3 DPI）**：`icon_button` 几何修正后
  `main_chrome` / `piano_roll_skeleton` 的工具栏间距收紧。已用「iced 归档图 /
  旧 egui / 新 egui」三方对照确认新布局更接近 iced 参考图（`target/diff-toolbar-3way.png`），
  差异只落在工具栏 band（dpi100 bbox `844..1391 × 49..77`），其余 6 个场景零改动。
- **无头验证**（`toolbar_controls_bands.rs`，5 用例）：走带三键与播放态 Pause、
  循环 / 撤销 / 重做 / 历史 / 录制、PPQ 进入编辑 + 输入 + 回车、
  自动滚动与协作、工具区 9 个动作顺序 + 命中区宽度一致性。
- **真机**（DPI-aware，窗口 2048×1232，工具栏 band 实测 `[248,30]-[2048,102]`）：
  逐项点击产出**恰好一条**动作：
  `Play → Pause → ToggleLoop → Undo → Redo → OpenHistory → ToolSelected(Pencil)
  → ToolCommand(Quantize)`；截图确认播放图标随状态切换、循环图标变 ↔、
  铅笔高亮、位置推进到 `00:01.83`。
- 验收：`lumino-ui` lib 845/845、`ui-egui` 全部集成测试（含新 5 用例）、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；`capture_egui` 12/12。

**Task 3.4（第九十三步）：删除 iced 工具栏整树（含最后一个 `iced_widget::canvas` 用户），高度拖拽迁 egui**

- **删除 iced `toolbar/view/` 整树（2010 行）**：`controls` / `status` /
  `detection_dashboard` / `onion_skin` / `tools` / `tools/arrangement` /
  `tools/curve_group` / `snap_icon`（最后一个 `iced_widget::canvas` 用户）/
  `toolbar_view` / `view.rs` 全部移除。
- **保留 `curve_tool_group`**：它被 `root/draw_toolbar.rs`（悬浮工具条，尚未迁移）
  复用，移动到 `toolbar/curve_tool_group.rs` 并保持 `pub(crate)` 导出。
- **`Toolbar::toolbar_view()` 改为同尺寸占位**：新文件 `toolbar/placeholder.rs`，
  高度 = `self.height`、底色 = `background.weakest`，只保证高度/底色与 egui band
  一致；同时把 `egui_snapshot` 从被删的 `detection_dashboard.rs` 迁到该文件
  （BPM / 时间 / 位置换算函数随迁，新增 `height` 字段）。
- **调用点收敛**：`piano_roll.rs`（2）、`arrangement.rs`（1）、
  `root_tests/speed_change.rs`（2）共 5 处改为无参调用；随之删除 3 处
  `ToolbarPerfContext` 局部构造与 2 处失效 import，`view_arrangement` /
  `right_content` 闭包的宽度参数改为 `_available_width`（工具栏不再需要预算）。
- **工具栏高度拖拽迁 egui**（原 iced `render_resize_handle` 在删除范围内）：
  - `ToolbarModel.height` + `ToolbarAction::{ResizeStarted, ResizeDragged, ResizeEnded}`；
  - 工具栏 band 底部 6px 手柄：按下/拖拽/松开经 `toolbar::Event::resize_*`
    回传，宽度跟随仍由宿主 `input.rs` 按光标 y 调用 `update_resize_position`；
  - band 高度改用 `model.height`，分组在 band 内**垂直居中**；生产
    `piano_roll.top_inset` / `tool_panel_top` / 右栏 `top_inset` 三处常量
    `TOOLBAR_H` 全部改为实时 `toolbar.height`（否则拖高后右栏与卷帘顶部重叠）。
- **`buttons.rs` 收缩为尺寸契约**：仅保留 `TOOL_SELECTOR_WIDTH` / `TOOL_BUTTON_WIDTH`
  （`overflow` 预算仍需要），删除 190 行视图工厂。
- **无头验证**（`toolbar_controls_bands.rs` +1 用例 = 6）：底边按下 →
  `ResizeStarted`、拖拽 → `ResizeDragged(y)`、松开 → `ResizeEnded`。
- **真机**（DPI-aware）：
  - 点击逐项仍各产生**恰好一条**：`Play / Pause / ToggleLoop`；
  - 拖底边向下 40px：`DEBUG toolbar band` 实测 `[[248,30]-[2048,102]]` →
    `[[248,30]-[2048,142]]`，日志 `ResizeStarted → ResizeDragged(98.4…128.8) →
    ResizeEnded`；截图确认分组垂直居中、钢琴卷帘从 142 开始、右栏图标列同起点，
    无重叠/缝隙（`target/tb7-top.png`）。
- 验收：`lumino-ui` lib 844/844、`ui-egui` lib 42/42 + 全部集成测试、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；`capture_egui` 12/12
  （**无需更新任何基线**：删除的是 iced 侧像素，egui 渲染未变）。
  触达 iced 的源文件 99 → **91**，`cargo tree` 仍为 39（待全部视图删除后收口）。

**Task 3.5（第九十四步）：音轨列表行点击 / 添加行迁 egui；修掉添加行重叠与「点名选不中」**

- **行命中**：`track_row` 由 `Sense::hover()` 改为 `Sense::click()`，点击回传
  `SidebarPanelAction::SelectTrack(track.id)`（**生产音轨 ID**，不是列表下标——
  重排后下标会错位）；`SidebarTrack` 新增 `id` 字段（生产 `track.id` 填充）。
- **添加行命中**：点击回传 `AddTrack`；行与添加行的矩形都 `extend_blocked`，
  避免 iced 的同位按钮再处理一次（表现为「一次点击加两条音轨」）。
- **新增动作通道** `chrome/sidebar/panel_action.rs`（一帧只保留第一个），
  生产侧 `egui_chrome/sidebar_panel.rs` 映射为
  `sidebar::Event::track_selected(id)` / `add_track()`；宿主 `actions.rs` 统一分发。
- **修复两个真实缺陷**：
  1. **音轨名吞点击**：egui 默认 `selectable_labels = true`，`Label` 会自己吃掉
     指针事件——点音轨**名字**选不中音轨（只有点行内空白有效）。名称标签改为
     `.selectable(false)`。
  2. **添加行与最后一行重叠**：原用 `TableBuilder` 逐行渲染，但表格实际行距
     ≈41px 而行高按 36px 计算，导致「添加音轨」行上移约 10px **压在最后一条
     音轨的下半部分**。改为手工逐行 + `ScrollArea`（`item_spacing.y = 0`），
     行距精确等于 36，添加行紧随其后。
- **基线更新（仅 2 场景 × 3 DPI）**：修掉重叠后添加行下移，diff bbox 实测
  `(56,126)-(234,166)` 只覆盖添加行区域；左侧对照（旧/新）确认音轨行完整、
  添加行独立不再压住上一行。其余 6 个场景零改动。
- **无头验证**（`sidebar_track_list_bands.rs`，4 用例）：三行点击各自回传
  音轨 ID（7/3/11，故意非 0..n）；点添加行**恰好**一条 `AddTrack`；
  行与添加行都在占用区；空白面板不产生动作。
- **真机**（DPI-aware）：
  - 点 A01 行 → `egui 音轨列表动作：SelectTrack(1)` 恰一条，选中高亮移动；
  - 点「添加音轨」→ `AddTrack` 恰一条，列表新增**一条**音轨（`A01 A01`），
    添加行完整位于列表下方（`target/tr2-crop.png`）。
- 验收：`lumino-ui` lib 844/844、`ui-egui` 全部集成测试（含新 4 用例）、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`；`capture_egui` 12/12。

**Task 3.7（第九十七步）：删除 iced 左侧栏整树（约 2100 行），只保留同宽占位**

- **删除**（9 个视图文件 + `panel/` 子目录）：
  `sidebar/panel.rs`、`sidebar/panel/{track_item,color}.rs`、
  `sidebar/{color_picker,numeric_picker,port_picker,channel_picker}.rs`、
  `sidebar/{context_menu,panel_context_menu}.rs`、`sidebar/route.rs`，
  以及三个仅测视图的 `panel_{color,port,channel}_tests.rs`。
- **`sidebar/view.rs` 改为同宽占位**：宽度 = 48 + 可见面板宽（`Sidebar::width()`），
  高度 Fill，无背景、无子控件；`view()` 签名收敛为无参，两处调用点
  （`piano_roll.rs` / `panels.rs` 瀑布流）同步。
- **`sidebar.rs` 只保留状态与事件处理**：`core` / `handling` / `track_reorder` /
  `view`；顺手删掉因视图消失而失效的 `ppq` / `note_precision` 局部变量与
  `widget::with_tooltip` 再导出。
- **菜单 Esc 关闭**：`menu.rs` 的音轨/面板菜单在背景点击之外也接受 Esc
  （iced 原 backdrop 可被 Esc 关闭；真机此前 Esc 无效）。
- **验证**：
  - `capture_egui` 12/12 且**零基线改动**——证明左栏像素已 100% 由 egui 覆盖，
    删掉 iced 绘制没有任何视觉变化；
  - 真机（DPI-aware）：选中行 `SelectTrack(1)`、右键 `OpenTrackMenu(1)`、
    拖手柄 `ResizeStarted/Ended`，面板右缘实测 `248 → 296`（拖 48px 即宽 48px），
    左栏始终正常显示；
  - 单测从 844 → **807**（−37 条均为被删视图的私有单测；egui 侧
    `sidebar_track_list_bands` 6 / `sidebar_menu_bands` 5 / `sidebar_picker_bands` 4 覆盖同等行为）。
- 验收：`lumino-ui` lib 807/807、`ui-egui` 全部集成测试、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`。触达 iced 的源文件 92 → **83**。

**Task 3.8（第九十八步）：删除 iced 标题栏菜单渲染（iced_aw MenuBar），只留同宽占位**

- **`titlebar/menu.rs` 拆成「数据 + 占位」**：保留 `MenuKind` / `MenuItem` /
  `MenuConfig` / `file_menu…menus()` / `display_name` / `event_display_name` /
  `view_placeholder`；删除 `view`、`menu_items`、`submenu_button`、`menu_button`、
  `base_button`、`disabled_button`、`button_template`、`base_split`
  （约 160 行）与 `MENU_WIDTH`。
- **`titlebar.rs` 两条渲染分支合并**：自绘标题栏与系统原生标题栏都直接用
  `menu::view_placeholder(language)`（此前只有 `overlay_enabled()` 时才走占位）；
  `export_material_enabled` 仅影响 egui 模型，iced 侧显式 `let _ =`。
- **移除 `iced_aw` 依赖**（`crates/editor/ui/Cargo.toml`），
  因为代码中已无任何 `iced_aw` 引用：
  `cargo tree -p lumino-ui | grep '^iced'` **39 → 32**。
- **诊断增强**：`chrome/menu.rs::show_labels` 在 `LUMINO_EGUI_DEBUG_RECTS=1` 时
  打印每个菜单标签的矩形。真机实测 `文件 48..93 / 编辑 93..138 / 视图 138..183 /
  帮助 183..228`——点 (110,15) 落在「编辑」是**正确**的（此前误判为错位）。
- 验证：`capture_egui` 12/12 零基线改动（叠加层开启时本来就渲染占位）；
  真机点「编辑」弹出撤销/重做/剪切/复制/粘贴/全选/查找，菜单命中与绘制一致。
- 验收：`lumino-ui` lib 807/807、`ui-egui` 全部集成测试、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`。触达 iced 的源文件 83 → **82**。

**Task 3.9（第九十九步）：删除 iced 悬浮绘制工具条整块（-1900 行）**

- **删除**：`root/draw_toolbar.rs`（1193 行，悬浮胶囊 + 设置面板 + 拖拽覆盖层）、
  `root/text_params_panel.rs`（文字参数面板，仅被它复用）、
  `toolbar/curve_tool_group.rs`（悬浮层 widget），以及随之失效的 4 个下拉渲染器
  `toolbar/{brush_dropdown,fill_division_dropdown,shape_dropdown,thickness_dropdown}.rs`
  （egui 侧 `chrome/tool_panel` 与 `toolbar_popup` 已覆盖同等 UI）。
- `piano_roll.rs` 去掉 `!overlay_enabled()` 的 iced 挂载分支；
  `root.rs` / `toolbar.rs` 清理对应 `mod` 与再导出。
- **3 个用例改测 egui 模型**：`draw_settings_panel` / `draw_thickness_entry` 里的
  「悬浮条仍应渲染」改为用主窗口 Root 断言 `egui_chrome_model().tool_panel.is_some()`
  （这两个用例的 Root 是对话框窗口，按设计不产出主窗外壳）；
  `tool_panel_switch` 的渲染冒烟同样改为模型断言。
- **验证**：
  - `capture_egui` 12/12 零基线改动（基线场景里悬浮条默认关闭）；
  - 真机（DPI-aware）：点工具栏「绘制入口」按钮 → `ToggleToolPanel`，
    底部胶囊正常出现（鼠标/曲线/颜料桶/画刷/形状/文字/齿轮），由 egui 独占绘制。
  - 单测 807 → **787**（−20 条均为 `draw_toolbar` 的私有渲染/居中用例；
    egui 侧 `tool_panel_bands` 5 条覆盖条目、设置面板与几何）。
- 验收：`lumino-ui` lib 787/787、`ui-egui` 全部集成测试、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`。触达 iced 的源文件 83 → **76**。

**Task 3.10（第一百步）：删除 iced 溢出菜单视图与录制按钮视图（-290 行）**

- **删除** `toolbar/overflow/view.rs`（186 行：面板网格、按钮样式、tooltip、
  定位函数）与 `toolbar/record.rs`（102 行）——egui 的 `chrome::overflow` 与
  `chrome::toolbar::controls` 已覆盖同等 UI，这两份 iced 视图在叠加层开启时
  永远不会被看到，却每帧仍在构建。
- `toolbar/overflow.rs` 收敛为 `state` + `interaction` + `items`（纯数据/折叠决策）。
- `root/view/right_content.rs` 从 80 行压到 20 行：溢出菜单覆盖层已归 egui，
  只保留 `responsive` 包装（参数改为 `_root/_has_selection/_arrangement_mode`）。
- 验证：`capture_egui` 12/12 零基线改动（大窗口无溢出，且叠加层本就覆盖）；
  `lumino-ui` lib 787/787、`clippy -D warnings`、`fmt --check`、`check --bin`。
  触达 iced 的源文件 76 → **74**。

**Task 3.11（第一百零一步）：删除 iced 音频导出进度对话框视图（死视图）**

- `DialogType::ExportProgress` 改为 `dialog_placeholder`（同尺寸底色），
  删除 `view/export_progress_dialog.rs` 与 `view.rs` 的模块声明。
- 依据 `dialog_size_hint`：该对话框尺寸已钉死 **406×233**，不依赖 iced 测量；
  注释亦标明「当前 Runner 侧无打开路径（死视图）」，内容由 egui 的
  `DialogModel::ExportProgress` 绘制。
- 验证：`capture_egui` 12/12 零基线改动、`lumino-ui` lib 787/787、
  `clippy -D warnings`、`fmt --check`、`check --bin`。
  触达 iced 的源文件 74 → **73**。

**Task 3.12（第一百零二步）：删除 3 个 iced 对话框视图，尺寸静态化**

- **尺寸静态化**（先取证再改）：
  `cargo test -p lumino-ui --lib debug_dump_dialog_window_sizes -- --ignored --nocapture`
  实测 `SaveConfirm 338×169`（内容下限）/ `LoadConfirm 420×233` /
  `ShapeDrawer 1000×700`。据此：
  - `SaveConfirm`：`auto()`（宽高皆测量）→ `pinned(380×180)`（取旧产品尺寸，
    留 egui 卡片内边距余量）；
  - `LoadConfirm`：`boxed(420)` + 测量高度 → `pinned(420×233)`；
  - `ShapeDrawer`：本就是 `pinned(1000×700)`。
- **删除**：`view/shape_drawer.rs`、`view/save_confirm_dialog.rs`、
  `view/load_confirm_dialog.rs`；`overlays.rs` 三处改 `dialog_placeholder`。
- **`DialogSizeHint::auto()` 删除**（已无使用者），4 条相关单测改为以
  `VideoExport`（唯一仍由测量驱动高度的对话框）验证「贴合内容 / 不超工作区 /
  测量不可用 → ZERO」，并断言**所有**对话框的宽度区间都已退化为一点。
- **真机**（DPI-aware）：画一个音符后 Alt+F4 → 未保存确认窗口出现，
  实测窗口 **475×225 物理 = 380×180 逻辑**，内容完整
  （标题栏 + 「是否保留未保存的更改?」+ 保存/关闭/取消），无裁剪。
- 验证：`capture_egui` 12/12 零基线改动、`lumino-ui` lib 787/787、
  `clippy -D warnings`、`fmt --check`、`check --bin`。
  触达 iced 的源文件 73 → **70**。

**Task 3.13（第一百零三步）：Toast 与首次使用帮助提示迁 egui**

- **新增 egui 浮层** `ui-egui/src/chrome/toast.rs` + 模型（`ToastCardModel` /
  `ToastModel` / `HelpTipModel`）：
  - Toast：右下角、避开状态栏 36、右边距 16；卡片内边距 10、圆角 6、间距 6、
    最大宽 420，图标 16 + 文案 14、白字；
  - 帮助提示：右下角内边距 16、卡片宽 360、内边距 14、圆角 8、1px 边框；
    标题 15 / 正文 12 / 「查看帮助」「知道了」两个按钮。
- **通道式接线**（与 mask_panel / video_preview 同模式）：`set_toast` /
  `set_help_tip` + `take_help_tip_action`；生产侧新增
  `egui_chrome/toast.rs`（`toast_model` / `help_tip_model` / 动作映射），
  宿主 `apply_overlay_actions` 统一分发。
- **删除 iced 视图**：`ToastManager::view`（改为 `egui_cards` 只给数据）、
  `view/help_tip.rs`、`piano_roll.rs` 两处叠加；`ToastLevel::icon` 的
  `✗`/`✓` 在 egui 默认字体中缺字（显示方框），改为 `×`/`√` 并更新单测。
- **调试开关**：`LUMINO_TOAST_DEMO=1` 启动即弹三条 Toast（正常入口需触发保存/错误）。
- **真机**（DPI-aware）：三条 Toast 右下角正确堆叠、蓝/橙/红配色与图标齐全，
  无方框缺字（`target/td-crop.png`）。
- 验证：`lumino-ui` lib 785/785、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。

**Task 3.14（第一百零四步）：删除 iced 设置树与关于页彩蛋视图；设置 crate 去 iced 化**

- **`lumino-ui-settings` 收敛为纯状态 crate**：删除 `pages/`（10 个设置页）、
  `components/`、`menu.rs` 与 `view()` / `render_content_area` / 样式函数，
  移除 `iced_core` / `iced_widget` **直接依赖**；保留 `SettingsPanel` 状态机、
  `panel/file_pickers`（系统文件对话框，与 iced 无关）与 16 条状态单测。
- **删除 `view/settings_dialog.rs`**：`DialogType::Settings` 改
  `dialog_placeholder`（720×540 钉死）；设置页内容由 `chrome::settings` 整窗绘制。
- **「关于」页 logo 彩蛋（UI-007）迁 egui**：新增
  `ui-egui/src/chrome/egg.rs`（`EggModel` + 旋转网格绘制，**不捕获鼠标**）与
  生产侧 `egui_egg_model()`（位置 / 旋转角 / 透明度 / 方盒尺寸原样搬运）；
  删除 `view/about_egg.rs` 与 `overlays.rs` 的顶层 Stack 叠加。
- **`MENU_INDEX_ABOUT` 绑定校验迁到真实消费侧**：
  `egui_chrome/dialogs.rs::settings_labels` 新增单测（中/英双语，索引 9 =
  「关于」且唯一），替代已删除的 iced 菜单表断言。
- **测量门禁同步**：`Settings` 加入 `layout_is_state_invariant`（iced 视图已退化
  为占位），`edge/tests` 的 corners 清单置空（已无真实 iced 对话框视图可挤）。
- **新增 `tests/egg_overlay.rs`（2 用例）**：飞行层绘制不 panic、
  **不进入占用区**（不吃设置页点击）；无模型时零开销。
- **验收**：`lumino-ui` lib 786/786、`lumino-ui-settings` lib 16/16、
  `ui-egui` 全部集成测试、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 **零基线改动**
  （设置对话框与 3 个变体都在基线里，证明删掉 iced 绘制后像素完全由 egui 提供）。
  触达 iced 的源文件 69 → **68**；`cargo tree -p lumino-ui | grep '^iced'`
  32 → **30**（`lumino-ui-settings` 不再直接依赖 iced）。

**Task 3.15（第一百零五步）：删除 iced 视频剪辑视图整树（view/video_clip/*），几何自算**

- **几何单源**：新增 `root/video_clip_math.rs`（纯计算，无 iced）：
  - `renderer_panel_layout(area)` 一次算出轨道卡 / 标题 / 16:9 预览 / 时间轴卡片 /
    标题行 / 画布 / 滚动条 / 设置卡（12 内边距、220 轨道列、12 列距、40 + 预览 +
    200 + 80、4 行距、8 时间轴内边距，逐项对齐 iced 原常量）；
  - `preview_size` / `renderer_preview_size`（16:9，最小 320×180）；
  - `ticks_to_seconds` / `duration_seconds` / `seconds_to_ticks` 与
    `PreviewGate`/`preview_gate`（原 `timeline.rs` / `preview.rs` 的纯逻辑）。
- **删除** `view/video_clip/{preview,timeline,layout,rect_probe}.rs` 与
  `video_clip.rs`（约 830 行），`rect_probe`（iced 布局探针）彻底退役。
- **渲染器入口面板改 egui 整块绘制**：新增
  `ui-egui/src/renderer_panel.rs` + `RendererPanelModel`（左轨道卡 / 标题 +
  缩放 / base 底 + 黑盒 + 1px 边 / 时间轴卡 / 设置卡三行），
  生产侧 `egui_chrome/renderer_panel.rs` 装配文案与导出设置摘要；
  `view_renderer_panel` 与 `view_waterfall_player` 改为铺满占位。
- **预览/时间轴改用自算矩形**：`video_timeline_model(area)` /
  `video_preview_model(window, content)` 不再读探针；后者同时把离屏纹理尺寸
  写回 `waterfall_player.size`（存储 = 显示）。
- **新增调试开关** `LUMINO_RENDERER_PANEL=1`（启动即进渲染器入口）。
- **真机**（DPI-aware，`LUMINO_EGUI_DEBUG_RECTS=1` 实测布局）：
  `timeline_title [[500,924]-[2028,952]]`、`clip_canvas [[500,956]-[2028,1084]]`、
  `preview [[529.8,86]-[1998.2,912]]`（16:9）；点播放 → 图标变暂停、
  位置 `0.0s → 1.6s → 2.5s`；拖动视频素材带 → 画布内容变化；
  左轨道卡 / 标题 / 预览黑盒 / 时间轴 / 设置卡全部在位，无错位、无重叠。
- 验收：`lumino-ui` lib **774/774**、`ui-egui` 全部集成测试、`clippy -D warnings`、
  `fmt --check`、`check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。
  触达 iced 的源文件 68 → **63**。

**Task 3.16（第一百零六步）：删除 iced 标题栏视图，拖动 / 双击 / 红绿灯迁 egui**

- **共享动作通道**：`chrome::titlebar` 新增 `TitlebarAction`
  （`Drag` / `ToggleMaximize` / `Minimize` / `Close`）与取用函数；
  主标题栏中间拖动区 `drag_started → Drag`、`double_clicked → ToggleMaximize`；
  三个红绿灯点击 → `Close` / `ToggleMaximize` / `Minimize`。
- **对话框 popup 标题栏**：新增 `chrome::popup_titlebar`（30px、聚焦/失焦底色、
  右侧 45×29 红绿灯、剩余区域拖动）。`ChromeModel` 新增
  `dialog_titlebar: Option<PopupTitlebarModel>`：
  - 除保存确认 / 设置页（自带标题栏，`DialogModel::draws_own_titlebar`）外，
    所有对话框在 `!use_native_titlebar` 时都由它接管；
  - 进度窗 / 设备警告窗同样补上（此前依赖 iced `view_popup`）。
- **生产映射**：新增 `egui_chrome/titlebar.rs`，把动作翻译成既有
  `window::Event::{drag, toggle_maximize, traffic_action}`；
  宿主 `apply_overlay_actions` 统一分发（含 `egui 标题栏动作` 日志）。
- **iced 标题栏退化为占位**：`titlebar.rs` 只保留 30px 同底色占位
  （`view` / `view_popup` 同签名），删除 `titlebar/logo.rs`、`titlebar/traffic.rs`；
  菜单数据与 `mode_toggle` 保留。
- **无头验证**（`titlebar_bands.rs`，4 用例）：主标题栏拖动 → `Drag`；
  三个红绿灯 → `Close`/`ToggleMaximize`/`Minimize`；popup 拖动与关闭；
  无 `dialog_titlebar` 时零动作。
- **真机**（DPI-aware，重建后）：
  `egui 标题栏动作：Drag` → 窗口矩形 `(-9,-9)-(2569,1549)` 变为
  `(-305,102)-(2255,1642)`；双击 → `ToggleMaximize`；
  点最右红绿灯 → `Close`，窗口关闭（`IsWindow=false`）。
- 验收：`lumino-ui` lib 774/774、`ui-egui` 全部集成测试（含新 4 用例）、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`、
  `capture_egui` 12/12 零基线改动（设置 / 保存确认自带标题栏，不受影响）。
  触达 iced 的源文件 63 → **61**；`titlebar/mode_toggle`、`menu::view_placeholder`、
  `widget.rs` 三个死视图/再导出随后删除，61 → **59**。
- **依赖收缩**：`crates/editor/ui/Cargo.toml` 移除**未使用**的 `iced_renderer`
  直接依赖（渲染上下文走 `iced_wgpu`），`cargo tree -p lumino-ui | grep '^iced'`
  **30 → 26**。

**Task 3.17（第一百零七步）：设置 crate 彻底摆脱 iced 传递依赖**

- **把两个无 iced 的模块迁到 `lumino-message`**：
  - `settings_event.rs`（设置面板事件枚举，仅依赖 `lumino-core`）；
  - `gpu_compat_state.rs`（GPU 检测 UI 状态，仅依赖 `lumino-message::events::window`）。
- **`lumino-ui-core` 保持原路径**：`pub use lumino_message::settings_event;`、
  `pub use lumino_message::gpu_compat_state;`——`lumino_ui_core::settings_event::Event`
  等既有引用无需改动。
- **`lumino-ui-settings` 去掉 `lumino-ui-core` 依赖**：`Event` / `OutputType` /
  `GpuCheck*` 改为直接从 `lumino-message` 引用；
  `cargo tree -p lumino-ui-settings | grep '^iced'` **12 → 0**。
- 验收：`lumino-message` / `lumino-ui-core` / `lumino-ui-settings` /
  `lumino-ui` 全部 `check --all-targets` 通过、`lumino-ui-settings` lib 16/16、
  `lumino-ui` lib 774/774、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。

**Task 4.1（原生运行时分支第一步）：`lumino-ui-runtime` 探针跑通**

- 新分支 `feat/egui-native-runtime`；新增 crate `crates/editor/ui-runtime`
  （不接入主程序，`cargo tree -p lumino-ui` 不受影响）。
- 运行时骨架：`winit 0.30` 事件循环 + `wgpu 27` Surface/Device/Queue +
  `egui 0.37` + `lumino-egui-bridge`（项目自有的 wgpu 27 绘制器）。
- 已打通：窗口创建、DPI（`set_pixels_per_point`）、Resize、鼠标移动/按键/滚轮、
  键盘基础键、纹理增量上传、清屏 + egui 图元提交、Surface present、60Hz 重绘。
- **真机探针**：`cargo run -p lumino-ui-runtime --bin egui-runtime-probe`
  窗口 1280×720，界面文字/按钮正常，连续点击按钮计数 `0 → 1 → 2`
  （截图 `target/probe2.png`），证明输入 → egui → wgpu 呈现整条链路可用。
- 验收：`cargo check/clippy -D warnings/fmt --check` 通过；
  主程序 `lumino-ui` lib 774/774、`check --bin lumino-rs`、
  `capture_egui` 12/12 零基线改动（新 crate 未接入主程序）。

**Task 4.2（下一步）**

1. 把 `NativeApp` 的窗口创建/事件循环替换进主程序 `src/runner`（先只接管主窗口，
   保留 iced 渲染路径做 A/B）；
2. 将 `Root::view` / `chrome::show_bands` 接入原生运行时：用 `egui::Context`
   直接跑 `chrome`（base 纹理 + band + 浮层），逐步停用 iced `UserInterface`；
3. 把 `ui-editor` 的命中/输入从 iced `UserInterface` 迁到 egui（音符点击/拖拽/框选），
   然后删除 `host/render/ui.rs` 与 `host/event/window/*` 的 iced 依赖；
4. 最后删除 `Theme`/`Renderer`/`Element` 别名与 `iced_*` 依赖，
   `cargo tree -p lumino-ui | grep '^iced'` 归零。

**Task 4.2–4.8（第一百零八至一百一十四步，同分支连续提交）**

- **4.2 生产外壳**：`lumino-ui` 新增 `Root::egui_chrome_model_native(viewport_width)`；
  运行时持有 `Root`，每帧 `chrome::show_bands` 渲染标题栏 / 工具栏 / 音轨列表 /
  右栏 / 状态栏（`1411e40f`）。
- **4.3 菜单回路**：`Root::apply_egui_menu_action(index)` 与 `Host` 共用
  `menu_action_message`，点「文件」弹出完整菜单（`49e106aa`）。
- **4.4 动作分发**：`Root::apply_egui_chrome_actions(ctx)` 统一处理菜单 / 工具栏 /
  左栏 / 状态栏 / 标题栏动作；真机点铅笔，选中高亮迁移（`2cead1e9`）。
- **4.5 gfx 画布**：运行时启动 `WgpuRenderThread`，离屏纹理注册到
  `Painter::set_external_texture(EDITOR_TEXTURE_ID)` 并作为 egui 底层
  （`d73df977`）。
- **4.6 编辑器输入**：内容区局部坐标 `Pressed/Moved/Released` → `Message::EditorAction`，
  不经过 iced `UserInterface`；Resize 时同步 `editor.set_canvas_size/offset`
  （`6350ed01`）。
- **4.7 音符显示（本步修复）**：两个根因都已定位并修复：
  1. **track_enc 缺失**：`NoteInstance.border_width` 高 16 位必须写
     `track_id + 1`（cull shader 据此区分主音轨 / 洋葱皮区间）；只写描边宽度
     （`track_enc = 0`）会让**所有音符被裁掉**——真机表现就是「日志说上传了 1 个
     音符，画布上什么都没有」。新增 `note_border_width()` + 单测锁死编码。
  2. **相机参数不同源**：`RenderParams` 之前用 `Default`（scroll 0 / zoom 0.1×20 /
     keyboard 60 / ruler 30 / ppq 1920），与编辑器视图（keyboard 120 / ruler 24 /
     ppq 480）不一致；现改为逐字段取自 `editor_state.view` / `canvas`，
     `canvas_offset` 同步为 `(SIDEBAR_W, TITLEBAR_H + toolbar.height)`。
  3. **协议顺序**：`TrackLayout` 不能再夹在 `BeginSession..Done` 之间（渲染线程按
     「交错」跳过并告警），改为 `Done` 之后单独补发；告警消失。
- **真机证据**（DPI-aware，`png` 像素包围盒量化）：铅笔拖出音符后，蓝色音符
  bbox = 逻辑 `(560.8, 406.4) - (943.2, 424.0)`，与「按下点 → 吸附后 tick 1920、
  松开点 → tick 5760、key 113、zoom 0.1×20、offset (248,102)」的解析解逐像素一致；
  画面中网格 / 键列 / 标尺 / 音符同图层对齐，无错位、无重叠。
- 验收：`lumino-ui` lib 774/774、`lumino-ui-runtime` 1/1、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`、
  `capture_egui` 12/12 零基线改动。

**Task 4.9（第一百一十五步）：钢琴卷帘右键菜单迁 egui**

- **条目表单一来源**：`lumino_message::context_menu` 新增
  `menu_items(target)` 与 `item_label(item)`——条目集合/顺序与 tooltip 文案
  由 message crate 维护，iced 与 egui 都只能取这一份（禁止各自复制）。
  三个目标：Notes 8 项 / DrawnShape 20 项 / PendingDrawnShape 12 项。
- **egui 绘制**：新增 `ui-egui::piano_roll_context_menu`：56px 宽图标条、
  40×40 行高、4px 行距、8px 内边距、深色面板（与 iced 常量逐项一致）、
  悬浮/按下白 alpha、右侧 tooltip（深底浅字）。**强制白色图标**
  （新增 `IconCache::paint_tinted`）：浅色主题下 `default_tint()` 会把图标
  染黑，在深色面板上几乎不可见（真机首版即此问题）。
- **关闭语义**：只认**左键**空白处关闭。首版用 `any_click()`，右键打开菜单的
  那次右键会被重放进同一帧并立刻触发 Close——真机表现为「菜单一闪而过/
  完全不出现」；改为 `primary_clicked()` 后正常。
- **输入占用**：菜单打开期间整块画布加入 `menu::blocked`（等价于 iced 旧
  `background_close_overlay`），关闭用的那一次点击不会同时编辑画布。
- **iced 侧退场**：删除 `ui-editor::context_menu::view` /
  `background_close_overlay` / 按钮/样式/图标映射（约 280 行），
  `rendering.rs` 的 ContextMenu 分支只保留内容层 + 原互斥语义；
  状态 `PianoRollContextMenuState` 保留（输入仍由 iced Canvas 产生）。
- **生产接线**：`Root::piano_roll_context_menu_model()`（canvas 局部坐标 →
  窗口坐标 + 可用区）与 `piano_roll_context_menu_message()`；
  `OverlayFrame` 新增 `piano_roll_menu`，`egui_overlay/actions.rs` 统一分发
  `Message::PianoRollContextMenu`。
- **无头验证**（`piano_roll_menu_bands.rs`，4 用例）：首行点击 → `Cut`；
  画布空白点击 → `Close`；打开期间画布进入占用区；DrawnShape 20 行几何不越界。
- **真机**（DPI-aware）：右键画布 → 深色图标条出现（8 项，图标白色清晰）；
  点「全选」→ 日志 `egui 卷帘右键菜单：ItemClicked(SelectAll)`；
  再次右键 + 点空白 → `Close`（首轮曾因 `any_click` 自关，已修）。
  截图 `target/prm6-menu.png`。
- 验收：`lumino-message` 53/53、`lumino-ui-editor` 908/908、
  `lumino-ui` lib 774/774、`ui-egui` 全部集成测试（含新 4 用例）、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`、
  `capture_egui` 12/12 **零基线改动**。

**Task 4.10（第一百一十六步）：编辑器 √× 按钮与框选框迁 egui（修复被底图覆盖的视觉）**

- **根因发现**：egui 叠加层把 gfx 离屏纹理作为**整窗底层**重绘，
  iced Canvas 的绘制内容（形状预览 / 框选框 / 悬浮按钮 / 文本框）全部被盖住——
  生产上这些元素一直「能点、看不见」。本轮把这些视觉逐个搬到 egui。
- **√× 悬浮按钮**（`ui-egui::editor_action_buttons`）：形状 / 图片转 MIDI /
  曲线 / 画刷四组共用 36×36、圆角 8、白 60% 边框、图标内缩 25%；
  几何仍由 `ui-editor` 的 `*_button_rects` 计算（单一来源），
  canvas 局部坐标 → 窗口坐标；点击回传 `EditorButtonAction`，
  生产映射为 `EditorAction::ShapeToolConfirm/LineToolConfirm/BrushConfirm/...`
  与 `RightSidebarAction::PlacementConfirm/Cancel`。
- **图标强制白色**（`IconCache::paint_tinted` 已在右键菜单引入）：
  浅色主题下深底按钮上的图标必须白，否则不可见。
- **框选框**（`ui-egui::marquee`）：拖拽中（Selecting，Direct / 弹簧动画）
  与选中音符包围盒；灰色 3px 边框 + 0.78 灰 35% 填充；按内容区裁剪；
  canvas 局部坐标 → 窗口坐标。
- **iced 侧退场**：删除四组 √× 的 Canvas 绘制与 `program_impl` 命中
  （避免同一击双触发），`confirm_buttons` 只保留 `BUTTON_SIZE`；
  删除横向 / 纵向 `selection_box::draw` 的 iced Geometry（避免与 egui 重复）。
- **输入占用**：按钮矩形加入 `menu::blocked`，按下按钮的那一击不会同时在画布落音符。
- **无头验证**（`editor_action_buttons_bands.rs`，3 用例）：确认 / 取消动作、
  按钮占用、空模型零占用。
- **真机**（DPI-aware）：
  - S 形状工具拖出待确认形状 → 绿色 √ / 红色 × 出现（截图 `target/eb-0-crop.png`）；
  - 点 √ → 日志 `egui 编辑器按钮：ShapeConfirm`，确认后的音符矩形可见
    （`target/eb2-1-confirmed-crop.png`）；
  - V 指针拖框选 → 灰色选框实时可见（`target/eb2-2-marquee-crop.png`）；
    松开后选中包围盒保留（`target/eb2-3-crop.png`）。
- 验收：`lumino-ui` lib 774/774、`lumino-ui-editor` 908/908、
  `ui-egui` 全部集成测试（含新 3 用例）、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。

**Task 4.11（第一百一十七步）：形状工具预览迁 egui（描边产物 + 引导轮廓）**

- **新增 `ui-egui::shape_preview`**：
  - 引导轮廓：固定 1.5px（与 iced `OUTLINE_GUIDE_WIDTH` 一致）；
  - 填充：主题主色 35% 透明（与 iced `FILL_ALPHA` 一致）；
  - 描边产物预览：`shape_outline_blocks`（与 √ 写入同源）逐块画实心矩形。
- **几何同源**：`effective_rect` + `shape_vertices`（圆用 64 段多边形逼近），
  canvas 局部坐标 → 窗口坐标；`ShapeInstance::spec()` / `ShapeToolState::preview_rect`
  直接复用，待确认图形与实时拖拽都覆盖。
- **生产 / 原生运行时接线**：`Root::shape_preview_model()` → `OverlayFrame.shape_preview`
  → `chrome::set_shape_preview` → `show_bands` 在框选框之上、√× 按钮之下绘制。
- **公开化**：`shape_outline_blocks` 改为 `pub`（egui 层复用同一份产物几何）。
- **真机**（DPI-aware）：S 形状工具拖拽过程中蓝色描边预览实时跟随
  （`target/sp-0-drag-crop.png`）；松开后待确认预览保留、√× 按钮叠加其上
  （`target/sp-1-pending-crop.png`）；点 √ 后音符与预览逐块一致
  （`target/sp-2-confirmed-crop.png`）。
  注：默认吸附精度为 1/4 音符（1920 tick），拖拽 200px 落在 0..1920 tick
  属正常吸附，预览 / 产物 / 按钮三者严格对齐（已用 debug 模型量化确认）。
- 验收：`lumino-ui` lib 774/774、`lumino-ui-editor` 908/908、
  `ui-egui` 全部集成测试、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。

**Task 4.12（第一百一十八步）：曲线预览与 I2M 区域框迁 egui**

- **新增 `ui-egui::curve_preview`**：
  - 曲线段：Corner 锚点走直线，其余走三次贝塞尔（`CubicBezierShape`），
    固定 1.5px 引导线；
  - 描边产物：`path_outline_blocks`（与 √ 写入同源）逐块实心矩形；
  - 锚点：半径 6 实心圆 + 白描边 2px，选中锚点外加一圈；
  - 控制柄：锚点→控制柄 1px 50% 辅助线 + 8×8 方块 + 白 1px 边框。
- **几何同源**：`line_tool.paths` / `AnchorBend` / `in_handle_abs` / `out_handle_abs`
  / `visible_handle_sides` / `selected_anchor` 直接取用；canvas 局部坐标 → 窗口坐标。
- **新增 `ui-egui::image_to_midi`**：Placing 阶段区域框，灰色 3px 边框 +
  0.78 灰 35% 填充，按内容区裁剪。
- **生产 / 原生运行时接线**：`Root::curve_preview_model()` / `i2m_region_model()`
  → `OverlayFrame` → `chrome::set_curve_preview` / `set_i2m_region`；
  绘制顺序：I2M 框 → 曲线预览 → 形状预览 → 框选框 → √× 按钮 → 右键菜单。
- **iced 侧退场**：`i2m_box::draw` 不再产出 Geometry（区域框与按钮均已迁）；
  `path_outline_blocks` 改 `pub` 供 egui 复用。
- **真机**（DPI-aware）：C 曲线工具点两下 → 蓝色产物块阶梯 + 细引导线 +
  锚点圆 + 控制柄方块全部可见（`target/cv-0-crop.png`），√/× 同时叠加。
- 验收：`lumino-ui` lib 774/774、`lumino-ui-editor` 908/908、
  `ui-egui` 全部集成测试、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。

**Task 4.13（第一百一十九步）：曲线填充预览与弯折按钮迁 egui**

- **填充预览**：`ui-editor` 新增无 iced 的 `FillPreview`（切分音符块 + 填充闭环）；
  egui 侧按块画半透明矩形，或按闭环用 `PathShape` 填充 + 轮廓描边。
- **弯折按钮**（`ui-egui::bend_buttons`）：52×20、圆角 4、11px 文本；
  当前态主题主色 + 白字 + 白边，另一态弱底 + 常规文字；两个按钮矩形
  加入 `menu::blocked`，不会同时命中画布。
- **生产接线补齐**：`Root::bend_buttons_model()` / `bend_button_message()`
  → `OverlayFrame.bend_buttons` → `actions.rs` 分发
  `Message::EditorAction(LineToolBend(...))`（此前只接了运行时，生产会丢动作）。
- **iced 侧退场**：删除 `line_tool_box` 的弯折按钮绘制与 `program_impl` 命中，
  `tr!` 导入移除。
- **无头验证**：
  - `ui` 单测 2 条：中间锚点 → 两枚按钮 + `LineToolBend` 映射；端点锚点 → 无按钮；
  - `bend_buttons_bands.rs` 3 条：两个按钮分别回传 Smooth/Corner、按钮占用、空模型零占用。
- **真机**（DPI-aware）：C 曲线两点 → 点线段中点插入中间锚点（自动选中）→
  「曲线（选中）/ 折线」按钮出现在锚点上方；点「折线」→ 日志
  `egui 弯折按钮：Corner`。截图 `target/bb5-crop.png`。
- 已知限制：矢量闭环用 egui `PathShape` 填充（凸多边形完全正确；
  凹多边形填充需后续自定义三角化），切分档位走音符块不受影响。
- 验收：`lumino-ui` lib 776/776、`lumino-ui-editor` 908/908、
  `ui-egui` 全部集成测试（含新 3 用例）、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。

**Task 4.14（第一百二十步）：文字工具覆盖层迁 egui（输入框 / 字形预览 / 三按钮）**

- **新增 `ui-egui::text_tool`**：
  - 文本框：灰色 3px 边框 + 0.78 灰 35% 填充；
  - 字形预览：生产侧新增 `Editor::text_tool_preview_rgba()`（与 √ 生成音符同一份
    `GlyphGrid` + `rasterize_glyph_alpha_for_tool`），egui 按签名缓存纹理并铺满框；
  - 输入框：框顶 28px `TextEdit::singleline`，编辑态自动聚焦；
  - √ / × / 模式按钮（绿 / 红 / 蓝 M·灰 N），矩形加入 `menu::blocked`。
- **新增消息**：`EditorAction::{TextToolConfirm, TextToolCancel, TextToolModeToggle}`，
  统一走 `Message::EditorAction`（此前 iced 画布直接调私有方法，egui 无入口）。
- **关键输入修复**：`host/event/window/events.rs` 在 egui `wants_keyboard` 时
  **跳过了 `handle_keyboard_input_event`** 的提前返回逻辑——原实现先跑工具快捷键
  再检查 `consumed_by_egui`，导致输入框里打 `b` 会切画刷、文本框直接消失。
  修复后裸字母快捷键让路，`a`/`b` 连续输入正常。
- **iced 侧退场**：`text_tool_input_overlay` 返回 `None`（隐藏的 iced TextInput 会
  抢键盘焦点、与 egui 各收一半按键）；`grid::text_tool_box` 模块与
  `Editor::text_tool_allowed` 公开化。
- **无头验证**（`text_tool_bands.rs`，4 条）：三按钮动作、输入 A → TextChanged、
  覆盖层占用、非编辑态只留文本框。
- **真机**（DPI-aware）：X 文字工具拖框 → 输入框 + 字形预览 + √/×/N 可见
  （`target/tt2-typed-crop.png`）；输入 `a`、`b` → `TextChanged("a")`、
  `TextChanged("ab")`，工具不再被 B 快捷键切走；点 √ → `Confirm`，
  生成三行文字音符（`target/tt2-confirmed-crop.png`）。
- 验收：`lumino-message` 53/53、`lumino-ui-editor` 908/908、
  `lumino-ui` lib 776/776、`ui-egui` 全部集成测试（含新 4 条）、
  `clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`、
  `capture_egui` 12/12 零基线改动。

**Task 4.15（第一百二十一步）：力度 / CC / Tempo 面板骨架迁 egui**

- **面板结构**：`ui-egui::velocity_panel` 绘制工具栏（模式按钮 / CC 下拉 / 右侧信息）、
  垂直网格（小节 / 拍 / 半拍）、水平网格与刻度标签、顶部 resize 手柄视觉。
  **数据曲线仍由 `lumino-gfx` 画进底层纹理**（力度柱 / CC 折线 / Tempo 折线），
  面板区域不铺背景——铺色会盖掉 gfx 曲线。
- **CC 下拉**：自绘 129 项滚动列表（Bend + CC0..127），弹层矩形精确加入
  `menu::blocked`，不会把点击漏给 iced 的隐藏 pick_list；数据区不占用，
  力度/CC 编辑仍由 iced Canvas 处理（本轮不动输入语义）。
- **动作**：`VelocityPanelAction::{ToggleMode, CcSelected}` → 生产
  `Message::Velocity(VelocityAction::...)`；`OverlayFrame.velocity_panel` +
  `actions.rs` + 原生运行时全部接线。
- **几何**：面板 = 画布下方 20px 滚动条起、高度 `visual.velocity_panel_height`；
  工具栏 28px；网格与刻度复用 `VelocityCanvas::velocity_to_y` /
  `bend_value_to_y` / `tempo_bpm_to_y` / `generate_tempo_levels`（与 iced 同源）。
- **无头验证**（`velocity_panel_bands.rs`，2 条）：模式按钮 → ToggleMode；
  工具栏占用、数据区不占用。
- **真机**（DPI-aware）：点左栏自动化路由 → 面板出现（工具栏 + 网格 + 刻度）；
  点模式按钮两次 → 日志 `egui 力度面板：ToggleMode` ×2，切到 CC 模式后
  CC 选择器显示 `1: Modulation Wheel`。截图 `target/vp3-0/1/2-crop.png`。
- 验收：`lumino-ui` lib 776/776、`lumino-ui-editor` 908/908、
  `ui-egui` 全部集成测试（含新 2 条）、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。

**Task 4.16（第一百二十二步）：力度面板编辑输入迁 egui（力度拖拽 + 曲线绘制）**

- **新增 `ui-egui::velocity_panel::input`**（无 iced 依赖，本地复刻
  `velocity_to_y` / `y_to_velocity` 公式）：
  - 命中力度点（10px）→ `VelocityDragStart/Move/End`；
  - 空白处按下 → `VelocityCurveStart`，拖动按起点/当前点线性插值生成
    `VelocityCurvePaint`（有选中集时只影响选中音符），松开 → `VelocityCurveEnd`。
- **职责边界**：仅在「力度模式 + 曲线工具」时 egui 接管数据区
  （`blocked` 排除顶部 5px resize 手柄）；Tempo / CC / Bend 的输入仍由
  iced Canvas 处理，resize 拖拽也保持原路径。
- **模型扩展**：`VelocityPanelModel` 新增力度点快照 / 选中集 / 视图参数
  （zoom_x / scroll_x / keyboard_width）/ `velocity_editing`。
- **动作映射**：6 个新 `VelocityPanelAction` → `Message::Velocity(VelocityAction::...)`。
- **真机**（DPI-aware）：A01 画一个音符 → 打开自动化面板 → 模式切到「力度」→
  C 曲线工具；拖动力度锚点向下，日志 `VelocityDragMove(0, 87→46)`、`VelocityDragEnd`，
  gfx 曲线锚点从 100 附近落到 46 附近（`target/ve2-before/after-crop.png`）。
- 验收：`lumino-ui` lib 776/776、`lumino-ui-editor` 908/908、
  `ui-egui` 全部集成测试、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。

**Task 4.17（第一百二十三步）：Tempo 输入与 Tempo 折线图迁 egui**

- **Tempo 输入**（`ui-egui::velocity_panel::input::handle_tempo`）：
  - 命中控制点（10px）→ `TempoDragStart/Move/End`；
  - 空白处左键 → `TempoAdd(snap_tick(x), y_to_bpm(y))`（吸附精度取
    `view.snap_precision`，与卷帘网格同源）；
  - 橡皮工具（Eraser / DrawEraser）点击控制点 → `TempoDelete(idx)`；
  - 右键控制点 → `TempoDelete(idx)`（与 iced 语义一致）。
- **滚轮**：面板内双轴滚轮 → `WheelScrolled { delta_x, delta_y }`
  （垂直 Pixels ÷50 换算档位，egui 0.37 用 `smooth_scroll_delta`）。
- **Tempo 折线图**（`velocity_panel/draw.rs`）：gfx 只画力度/CC 曲线，
  Tempo 由 egui 补——折线 + 4px 锚点 + BPM 文本 + **最后一个点向右无限延伸**
  （BPM 恒定），颜色 `#338CFF`，线宽取 `automation_line_thickness`。
- **职责边界**：仅 `Tempo + Curve/Eraser/DrawEraser` 时 egui 接管数据区；
  `blocked` 排除顶部 5px resize 手柄，CC/Bend 输入与 resize 仍走 iced。
- **拆分**（保持单文件 < 400 行）：`velocity_panel.rs` 364 行、
  `types.rs` 45、`draw.rs` 66、`input.rs` 291。
- **真机**（DPI-aware）：自动化面板默认 Tempo 模式 + C 曲线工具 →
  蓝折线 + 默认 120 BPM 锚点可见；拖动锚点向下 → 日志
  `TempoDragMove(0, 30.79)` ×N + `TempoDragEnd`，折线随之跌到 30 BPM
  （`target/tp2-before/after-crop.png`）。
- 验收：`lumino-ui` lib 776/776、`lumino-ui-editor` 908/908、
  `ui-egui` 全部集成测试、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。

**Task 4.18（第一百二十四步）：CC / Bend 自动化输入迁 egui**

- **新增 `velocity_panel/input/automation.rs`**（拆出以保持 < 400 行）：
  - 命中锚点（10px）→ `AutomationMove`；首个移动 `first = true`（生产侧记一次历史），
    后续走 `AutomationBatch`，避免 60fps 拖拽塞满撤销栈；
  - 空白处左键 → `AutomationAdd`（`controller = None` 即 Pitch Bend；值按
    gfx `value_to_y` / `y_to_value` 互逆映射，tick 走 `snap_precision`）；
  - 橡皮工具点击锚点 / **右键锚点** → `AutomationDelete`；
  - 面板内滚轮 → `WheelScrolled`。
- **模型**：`AutomationLaneModel`（track_idx / lane_idx / controller / max_value /
  anchors）在 `egui_chrome/velocity_panel/automation.rs` 构建（CC 取 127 上限，
  Bend 取 16383）；`automation_editing` 仅 CC/Bend + Curve/Eraser 时为真。
- **职责边界**：命中时数据区加入 `menu::blocked`（resize 手柄仍归 iced）；
  Tempo / Velocity 已有各自的输入分支，互不抢占。
- **拆分**：`ui-egui/velocity_panel.rs` 338 行 + `action.rs` 76 + `input.rs` 307 +
  `input/automation.rs` 180 + `draw.rs` 66 + `types.rs` 77；
  `ui/.../velocity_panel.rs` 398 + `velocity_panel/automation.rs` 47。
- **验证**：
  - 无头（`velocity_panel_bands.rs`，4 条）：右键锚点 → `AutomationDelete`；
    空白点击 → `AutomationAdd`（CC 值 ≤ 127、tick 吸附 1920）；模式按钮；占用区。
  - 真机（DPI-aware，Bend 模式）：空白点击新增锚点
    `AutomationAdd { tick: 1920, value: 9669 }`；拖动 → 76 条
    `AutomationMove`（`first: true` 仅首条）；右键锚点 →
    `AutomationDelete { lane_idx: 0, tick: 1920 }`。
- 验收：`lumino-ui` lib 776/776、`lumino-ui-editor` 908/908、
  `ui-egui` 全部集成测试（含新 2 条）、`clippy -D warnings`、`fmt --check`、
  `check --bin lumino-rs`、`capture_egui` 12/12 零基线改动。

**Task 4.19（第一百二十五步）：力度面板 resize 拖拽迁 egui**

- **新增 `velocity_panel::input::ResizeState` / `resize_zone` / `handle_resize`**：
  命中区 = canvas 顶端 5px；按下记 `(start_y, start_height)`，拖动按
  `new_height = start_height + (start_y - y)` 计算并钳制 60..400，与当前高度差
  > 1px 才发 `Resize(height)`（与 iced `handle_cursor_moved` 同公式、同阈值）；
  悬停未按下时设 `CursorIcon::ResizeVertical`。
- **动作链**：`VelocityPanelAction::Resize(f32)` → `Message::VelocityPanelResize`，
  与 iced 侧同名消息汇合。
- **优先级**：`handle_resize` 在数据区编辑分支**之前**执行，且 resize 区常驻
  `blocked`；命中后本帧不再分派力度 / Tempo / CC / Bend 输入，避免拖手柄时
  顺带改数据。
- **拆分**：`velocity_panel.rs` 340 行、`input.rs` 376、`input/automation.rs` 178、
  `action.rs` 88、`ui/.../velocity_panel.rs` 399。
- **验证**：
  - 无头（`velocity_panel_bands.rs` 第 5 条）：手柄上拖 40px → `Resize(190)`
    （150 高面板 + 40，误差 < 2px）。
  - 真机（DPI-aware，Bend + Curve）：canvas 由 `[[248 1090]-[1980 1212]]` 变为
    `[[248 1044.4]-[1980 1212]]`，日志 10 条 `Resize(154.8 … 195.6)`，
    面板变高、工具栏与状态栏无重叠（截图 `target/rsz-panel.png`）。
- 验收：`lumino-ui` lib 776/776、`lumino-ui-editor` 908/908、`ui-egui` 全部集成
  测试（含新 1 条）、`clippy -D warnings`、`fmt --check`、`check --bin lumino-rs`、
  `capture_egui` 12/12 零基线改动。

**Task 4.20（下一步）**

1. 切主窗口入口到 `lumino-ui-runtime`：egui overlay 增加可选 clear，
   非对话框主窗口跳过 iced `present`，用环境变量保留回退；
2. 删除 `host/render/ui.rs` 与 `host/event/window/*` 的 iced 输入路由；
3. 曲线矢量填充的凹多边形三角化；
4. 摘 `Theme`/`Renderer`/`Element` 与 `iced_*`，`cargo tree` 归零。
