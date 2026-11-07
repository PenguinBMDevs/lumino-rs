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

**Task 1.3（下一步）：工具栏下拉 / 提示迁移**

- 现状：标题栏（含菜单/子菜单）、工具栏、左侧栏、右侧栏图标列、状态栏、
  保存确认对话框、设置「常规」页（深/浅/英文，小窗口可滚）已由 egui 绘制；
  工具栏下拉（吸附 / 洋葱皮 / 溢出 / 画笔 / 粗细 / 形状 / 文字参数 / 工具面板）
  与其余设置页仍 iced——这是 `has_iced_popup` 让位逻辑仅剩的来源。
- 下一步：逐块迁移工具栏下拉并改为 egui 弹层，随后可删除整块让位；
  再逐页建模其余设置页与右侧栏面板内容。
- 卷帘骨架仍由 `lumino-gfx` wgpu 通道拥有，等该通道拆分/迁移后再打开
  `LUMINO_EGUI_PIANOROLL`。
