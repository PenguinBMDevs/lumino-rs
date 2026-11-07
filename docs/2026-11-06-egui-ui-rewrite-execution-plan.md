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

**Task 1.3（第二步）：工具栏接管与输入路由**

- 现状：标题栏 / 状态栏已由 egui 用真实状态绘制；四个场景的错位/重叠已清零；
  工具栏 / 侧栏 / 钢琴卷帘在场景层已对齐，但尚未接入真实窗口状态。
- 下一步：按同样方式接管工具栏（只读 + 命中热区先不消费事件），随后接入指针/键盘
  路由；`lumino-gfx` 的 wgpu30 迁移继续等待 iced 退出渲染路径。
