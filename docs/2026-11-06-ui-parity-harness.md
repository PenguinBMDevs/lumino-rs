# iced 视觉基线工具链（UI Parity Harness）

> 日期：2026-11-06  
> Crate：`crates/editor/ui-parity`（`lumino-ui-parity`）  
> 目的：为后续 UI 重写（egui 草稿 / 重构）提供**可重复**的 iced 侧视觉参照。

## 1. 它能做什么

- 固定窗口尺寸、主题、语言、DPI，渲染 4 个代表性场景；
- 生成并校验 `tests/ui-baseline/iced/` 下的 12 张 PNG（4 场景 × 3 DPI）；
- 提供 `ui-diff`：逐像素统计 + 红/蓝差异热图 + JSON 报告。

**本 crate 不修改任何生产 UI crate，也不参与正式应用打包。**

## 2. 固定条件

| 条件 | 取值 |
| --- | --- |
| 主题 | iced 内置 `Tokyo Night Storm`（`UiConfig::default()` 本身是 Light，工具显式覆盖） |
| 语言 | `Language::ZhCn`（zh-CN） |
| 标题栏 | 自制标题栏（`use_native_titlebar = false`） |
| 字体 | `lumino_ui_core::font::init(&UiConfig::default())`，与生产启动一致（默认 `Microsoft YaHei`，缺失时由 cosmic-text 回退） |
| 光标 | `Cursor::Unavailable`（不产生 hover） |
| 帮助提示 | 显式 `dismiss()` |
| 菜单 | 关闭（`is_menu_open = false`） |
| 模式切换动画 | 复位到静止态 |
| FPS | 固定 `60.0`（通过 `Message::Window(FpsUpdate(60.0))` 同时同步状态栏与 Window 状态） |
| Toast | 默认空队列 |

### 场景清单

| 场景 | 逻辑尺寸 | 说明 |
| --- | --- | --- |
| `main_chrome` | 1600×1000 | 主窗口：标题栏、工具栏、左右侧栏、钢琴卷帘、力度面板、状态栏 |
| `settings_general` | 1200×800 | `Root::new_settings_dialog`，`selected_menu_index = 0`（常规页） |
| `save_confirm_dialog` | 560×260 | `DialogType::SaveConfirm`，`is_open = true` |
| `piano_roll_skeleton` | 1600×1000 | 空工程钢琴卷帘骨架；滚到第 9 小节、`zoom_x = 0.05`，与 `main_chrome` 区分 |

空工程不包含音符：公开 API 无法安全同步左侧栏音轨列表（`Root.sidebar` 为
`pub(crate)`），强行写 `MidiDocument` 会造成“文档有轨、侧栏只有 Conductor”的
不一致。

> **重要：钢琴卷帘的网格与音符不在 iced 元素树内。** 生产运行时，网格/音符由
> `Host` 的 wgpu 渲染通道先画到底层帧，再叠加 iced 的标题栏、工具栏、键盘、标尺、
> 滚动条等 UI。本工具只截 iced 元素树，因此 `piano_roll_skeleton` 看到的是
> **骨架层**（键盘、标尺、播放指示线、滚动条、力度面板），中央编辑区为纯背景，
> 不含宿主渲染的网格线/音符。这是真实分层结果，不是空图或伪造图。

### DPI

- 逻辑窗口尺寸**不变**；
- 物理输出为 `logical × {1.0, 1.25, 1.5}`；
- 命名：`main_chrome-dpi100.png` / `-dpi125.png` / `-dpi150.png`。

这与 iced 的 HiDPI 语义一致：布局用逻辑像素，文字/几何按物理像素光栅化。

## 3. 生成基线

```bash
# 无参数：生成全部 4 场景 × 3 DPI 到 tests/ui-baseline/iced/
cargo run -p lumino-ui-parity --bin capture_iced

# 自定义输出目录（调试用）
cargo run -p lumino-ui-parity --bin capture_iced -- --out-dir target/ui-baseline
```

- 文件不存在：创建并返回 0；
- 文件已存在：逐字节比对；全部一致返回 0，不一致打印路径并返回非零；
- 第二次运行应返回 0（确定性验证）。

需要 GPU（`iced_wgpu` 无头适配器）。无 GPU 环境会以
`无法创建 iced_wgpu 无头渲染器` 失败。

## 4. 对比两张图

```bash
cargo run -p lumino-ui-parity --bin ui-diff -- \
  --a tests/ui-baseline/iced/main_chrome-dpi100.png \
  --b target/egui-main_chrome-dpi100.png \
  --out target/diff-main_chrome-dpi100.png \
  --color-tolerance 2 \
  --geometry-tolerance 1
```

JSON 关键字段：

| 字段 | 含义 |
| --- | --- |
| `offset_x` / `offset_y` | 几何容差搜索出的全局平移（B 的 `(x,y)` 对齐 A 的 `(x+dx,y+dy)`） |
| `pixels_over_tolerance` | 任一通道差 > `color_tolerance` 的像素数（含边界外无法比较的像素） |
| `pixels_over_tolerance_ratio` | 上述像素占全图比例 |
| `uncovered_pixel_count` | 平移后落在边界外、无法比较的像素数 |
| `different_pixels` | 任一通道存在差异（delta > 0）的像素数 |
| `max_channel_delta` / `mean_channel_delta` | 重叠区域内单通道最大 / 平均绝对差 |
| `matched` | `pixels_over_tolerance == 0` |

差异热图规则：

- 黑：在容差内（或完全相同）；
- 红：A 比 B 亮；
- 蓝：B 比 A 亮；
- 紫：同一像素两个方向都有差异；
- 灰：平移后无法比较的边界像素。

强度按 `delta × 4` 放大，方便观察 1~3 的细微抗锯齿差异。

`--fail-on-diff` 可在存在超容差像素时返回非零，适合 CI。默认只报告、不失败。

## 5. 已知差异 / 限制

1. **`iced_test 0.14.0` 无法输出真实多档 DPI**  
   其 `Simulator::snapshot` 把 `scale_factor` 硬编码为 `2.0`。本工具因此直接驱动
   `iced_runtime::UserInterface` + `iced_wgpu::Renderer::screenshot`，复刻同一无头
   路径但把 scale 作为参数。`iced_test` 保留用于 2x 交叉校验：
   ```bash
   cargo test -p lumino-ui-parity -- --ignored
   ```
   该测试要求 GPU，普通 `cargo test` 不会执行。

2. **字体**  
   基线使用生产默认字体解析链（`UiConfig::default()` → `Microsoft YaHei`）。
   若机器未安装该字体，cosmic-text 会回退到系统字体，PNG 会与其它机器不同。
   跨机器比对前请确认字体环境一致。

3. **抗锯齿 / MSAA**  
   `iced_wgpu` 无头渲染启用 MSAA×4；不同 GPU 厂商/驱动、不同 `WGPU_BACKEND`
   可能产生 1~2 的通道差。`ui-diff` 的容差与统计正是为此设计，不要求逐像素全等。

4. **设置页字体扫描**  
   `settings_general` 构建时会调用 `lumino_note_core::font_scanner::get_cached_fonts()`
   （生产同路径）；首次运行可能较慢，但只影响耗时，不影响确定性。

5. **未加入音符 / 宿主渲染层缺失**  
   `piano_roll_skeleton` 是空工程骨架；更重要的是，网格线/音符由 `Host` 的 wgpu
   通道渲染，不在 iced 元素树内，因此本工具的截图不含它们。后续若需要“有音符”
   基线，应在宿主层驱动完整装载与渲染，或给 `Root` 增加安全的音轨同步 API。

6. **不提交临时文件**  
   `target/`、对比过程产生的 diff PNG 均不应提交；仅提交
   `tests/ui-baseline/iced/*.png`。

## 6. 验收命令

```bash
cargo fmt
cargo check -p lumino-ui-parity
cargo test -p lumino-ui-parity
cargo clippy -p lumino-ui-parity --all-targets -- -D warnings
cargo run -p lumino-ui-parity --bin capture_iced   # 首次：创建 12 张
cargo run -p lumino-ui-parity --bin capture_iced   # 第二次：返回 0
```
