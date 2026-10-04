# Spiderweb vs Lumino 音符绘制工具对比调研

> 调研对象：`../Spiderweb`（Python/Tkinter，UnPrioritized/Spiderweb，MIT）
> 对比对象：`lumino`（Rust，`crates/editor/*`）
> 结论口径：**「缺失」= Spiderweb 有、Lumino 没有对应能力**；证据均落到源码文件/符号。

---

## 一、结论摘要

1. **Spiderweb 是一套「把图形画成音符」的完整创作套件**，核心是 11 个绘制图元 + 一套**形状后处理链**（填充 / 突起 / 爪机 / 扫弦 / 公式图案 / 合成 / 合并切割 / 对称 / 拉直 / 分通道）。
2. **Lumino 目前只覆盖了「图元 + 填充」的下半段**：曲线（贝塞尔）、画刷、形状（矩形/圆/三角）、文字、颜料桶填充、力度编辑器；图元渲染层基本对齐，但**上半段的「形状后处理 + 黑乐谱专用生成器」几乎全空**。
3. **关键事实（移植谱系）**：Lumino 的**音符生成内核已经直接移植自 Spiderweb**——
   - `line_tool/paths.rs` ← 移植自 `scripts/notes/paths.py`（逐音高行精确跨越、无缝连奏）
   - `line_tool/fill/spans.rs` ← 移植自 `scripts/notes/custom.py`（`poly_edges`/`row_spans`/`merge_spans`、even-odd 内部填充）
   - 换言之：**Lumino 拿到了 Spiderweb 的「引擎」，但没拿到它的「工具集」**。Spiderweb `notes/` 下 24 个模块，Lumino 只搬了 2 个。
4. **Lumino 反向超出 Spiderweb 的一点**：图像转 MIDI（i2m）、视频/音频离线导出等（本报告聚焦绘制工具，仅备注）。

---

## 二、Spiderweb 工具全景（证据）

### 2.1 绘制图元（`window/app.py:68-73` `TOOLS` / `SHAPE_TOOLS`）

| 工具 | 键 | 说明 | 实现 |
|---|---|---|---|
| Select | V | 选择/移动形状、编辑锚点 | `roll/pianoroll.py` |
| Line | L | 直线（拖拽或两点点击） | `notes/paths.py` |
| Polyline | P | 多段折线 | `notes/paths.py` |
| Freehand | F | 自由手绘（带 Straighten 拉直） | `notes/smooth.py` |
| Curve | C | 贝塞尔（锚点+手柄，中键加锚点） | `notes/bezier.py` |
| Arc | A | 三点圆弧 | `notes/arc.py` |
| Custom shape | S | 形状库放置 | `notes/custom.py` |
| Circle / Polygon | O / Q | 圆、多边形/星形/交叉星形 | `notes/polygon.py` |
| Funnel | N | 漏斗 | `notes/funnel.py` |
| Text | X | 文字转音符 | `notes/text.py`+`notes/fonts.py` |
| Hz bass | H | 超快 spam 形成音调 + 预览 | `notes/hzbass.py` |

### 2.2 形状合成与编辑（`help_texts.py` 主题表）

| 能力 | 触发 | 说明 | 实现 |
|---|---|---|---|
| Live shape | G | 多图元画进**一个**可填充形状 | `roll/roll_live.py` |
| Turn into live shape | Ctrl+L | 选中图元 → 合成一个 live shape（可逆 Ctrl+Shift+G） | `notes/convert.py` |
| Custom edit | — | 缩放/旋转/倾斜/移动 | `roll/roll_custom.py` |
| Curves pen | — | 锚点/手柄编辑，Alt=尖角 | `roll/roll_curve.py` |
| Symmetric halves | 右键曲线 | 半边镜像（拱形）/旋转（S 形） | `notes/bezier.py` |
| Join / Split | Ctrl+G / 右键 | 多个图元合并成一条曲线 / 原地切两半 | `notes/joined.py` |
| Formula | 右键线/曲线 | 形状(Circle/Spiral/Heart…)+图案(Wave/Zigzag/Square wave/Spikes…) | `notes/pattern.py` |
| Straighten | 面板 | 手绘拉直 0=原样 → 越大越直 | `notes/smooth.py` |
| Sharing | Ctrl+C | 形状序列化为**一行文本**分享 | `files/share.py` |

### 2.3 填充与黑乐谱生成器

| 能力 | 说明 | 实现 |
|---|---|---|
| Inside fill | Empty / **Fill** / **Spam** / **Outline spam** | `notes/custom.py` |
| Ends | 余数处理：keep/min/round/drop × centred/stretch | `notes/custom.py` |
| Outline | 轮廓单独占一个通道 | `notes/custom.py` |
| **Channels** | As drawn / Single / Multi（重叠自动分通道，每通道一轨） | `notes/engine.py` |
| **Tumours** | 沿线的肿瘤状突起（size/length/distance/rotation/side + graph 沿线变化） | `notes/tumour.py` |
| **Claw machine** | 切时间片 / 保留丢弃部分音符 | `notes/claw.py` |
| **Strum** | 扫弦：和弦音符依次错开起始（直线→斜坡/曲线） | `notes/strum.py` |
| Envelope | 力度包络 | `notes/envelope.py` |
| Velocity pane | Linear / Curve / **Pencil** / **Formula** 四种绘制 | `window/velocity.py`+`velocity_formula.py` |

### 2.4 工程体验

- **Drawer 形状抽屉**：自绘形状 + 形状库（`scripts/shapes/*.json`，`window/drawer.py`）
- **History 面板**：命名撤销步骤列表（`window/history.py`）
- **Numbers**：数字框支持数学表达式（`960*4`），`files/mathexpr.py`
- **Help**：F1 可搜索帮助 + 首次使用 tip + 演示 GIF（`help_texts.py`）
- **Snap picker**：自定义吸附图片（`window/snap_picker.py`）
- **Domino 剪贴板互通**：`files/domino_clip.py`
- Text 参数：字体/字号/行距/字距/阈值/grow 膨胀（`window/panel_text.py`）

---

## 三、Lumino 现有绘制能力（证据）

| 能力 | 实现位置 | 覆盖 Spiderweb 的 |
|---|---|---|
| Curve 工具（三次贝塞尔、锚点+in/out 手柄、多路径） | `ui-editor/src/interaction/line_tool.rs` + `line_tool/{geom,paths,hit_test,confirm}.rs`；状态 `editor-state/.../line_tool.rs` | Line / Polyline / Curve（多段折线可画，但无独立 Line/Polyline 工具） |
| 颜料桶填充（内部填充 + x 分音符切分，even-odd 逐音高行） | `ui-editor/src/interaction/line_tool/fill.rs` + `fill/spans.rs` | Inside fill（仅 Fill 模式） |
| Brush 画刷（自由手绘矢量笔画，粗细 1–20，逐层音轨分配） | `ui-editor/src/interaction/brush/{,.rs,cells,confirm}.rs`；`core/core/src/types.rs:160` `BrushConfig` | Freehand（**无** Straighten） |
| Shape 形状（矩形 / 圆(非 Shift 为椭圆) / 三角形） | `ui-editor/src/interaction/shape_tool.rs`；`editor-state/.../shape_tool.rs` `ShapeKind` | Circle 的部分 |
| Text 文字（字形光栅化；Normal / KeyRangeMerged 两模式） | `ui-editor/src/interaction/text_tool/{,.rs,font,rasterize,tool}.rs` | Text（**无**字体/字号/间距/阈值/grow 参数） |
| 力度/CC 编辑器（力度点拖拽 + 线性曲线；Tempo/Bend/CC 自动化） | `ui-editor/src/velocity/`（`widget/sections/handling/{velocity,automation,tempo,bend}.rs`） | Velocity pane（仅点+线性，**无** Pencil/Formula/Envelope） |
| 图像转 MIDI | `ui/src/right_sidebar/convert.rs`、`editor-state/.../image_to_midi.rs` | **Spiderweb 无**（Lumino 独有） |
| 量化 / 变速 / 垂直·水平翻转 / 移调 / 分割 / 合并 / 连奏 | `ui/src/root/handlers/toolbar/tools/{note_ops,note_group_ops}.rs` | 音符级通用操作 |
| 剪贴板（LUMC）+ Domino 互通 + 撤销/重做 | `audio/midi-model/src/clipboard{/,/domino}.rs`、`ui/src/host/event/keyboard.rs` | Selecting / Domino |

**工具枚举**：`core/core/src/types.rs:33` `enum Tool { Pointer, PointerYSelect, Pencil, Brush, Pen, Eraser, DrawEraser, Razor, Curve, Shape, Text }`（`Pen`/`Razor` 为未接线占位）。
**绘制悬浮条**：`ui-core/src/toolbar_event.rs:158` `ToolPanelItem { StrokeSettings, Curve, FillBucket, Brush, Shape, Text }`。

---

## 四、Lumino 缺失清单（按优先级）

### 🔴 P0 — 黑乐谱创作核心的「形状后处理 / 生成器」，完全缺失

| # | 缺失能力 | Spiderweb 实现 | Lumino 现状 | 影响 |
|---|---|---|---|---|
| 1 | **Tumours 沿途突起** | `notes/tumour.py` | 全仓 `tumour/tumor` 命中 **0** | 黑乐谱招牌纹理，无替代 |
| 2 | **Claw machine 爪机** | `notes/claw.py` | `claw` 命中 **0** | 无法对已生成音符做切时间片/筛选 |
| 3 | **Strum 扫弦** | `notes/strum.py` | `strum` 仅 "program change" 假命中 | 和弦错开起始无法实现 |
| 4 | **Formula 公式图案** | `notes/pattern.py` | `formula`/`spiral` 命中 **0** | 无法沿曲线/线条铺 Wave/Zigzag/Spiral/Heart 等图案 |
| 5 | **Funnel 漏斗** | `notes/funnel.py` | `funnel` 命中 **0** | 整类工具缺失 |
| 6 | **Hz bass** | `notes/hzbass.py` | `hzbass`/`hz_bass` 命中 **0** | 无法用极快 spam 合成音调 |
| 7 | **Live shape 合成 / Turn into live shape** | `roll_live.py`、`notes/convert.py` | `live shape` 命中 **0** | 无法把多个图元拼成一个可填充形状 |
| 8 | **形状级 Join / Split** | `notes/joined.py` | 现有 Split/Glue 是**音符级**，无形状级 | 不能合并/切割整条曲线 |
| 9 | **Arc 三点圆弧工具** | `notes/arc.py` | 无独立 Arc 工具（只能用贝塞尔近似） | 画标准圆弧低效 |
| 10 | **Channels 重叠自动分通道** | `notes/engine.py` | 无自动分通道逻辑 | 重叠音符无法一键铺到多轨 |

### 🟠 P1 — 明显影响表现力 / 效率

| # | 缺失能力 | Spiderweb 实现 | Lumino 现状 |
|---|---|---|---|
| 11 | **Polygon / Star / Crossing star** | `notes/polygon.py` | `ShapeKind` 仅 `Rectangle/Circle/Triangle` |
| 12 | **Custom shape 形状库 + Drawer 自绘形状** | `window/drawer.py`、`scripts/shapes/*.json` | 无形状库、无形状持久化/复用 |
| 13 | **Straighten 手绘拉直** | `notes/smooth.py` | 画刷无拉直/平滑 |
| 14 | **Symmetric halves 对称半边** | `notes/bezier.py` | `symmetric` 命中 **0** |
| 15 | **填充细分模式**：Spam / Outline spam / Ends / Outline 独立通道 | `notes/custom.py` | 仅「内部填充 + 分音符切分」，无 spam 背靠背、无 Ends 处理 |
| 16 | **力度编辑的 Pencil / Formula 模式 + 包络** | `velocity.py`、`notes/envelope.py` | 仅点拖拽 + **线性**曲线 |
| 17 | **形状旋转 90°（Ctrl+←/→）** | `roll/pianoroll.py` | 有翻转，无 90° 旋转 |

### 🟡 P2 — 体验 / 工程

| # | 缺失能力 | Spiderweb 实现 |
|---|---|---|
| 18 | **工具单键快捷键**（V/L/P/F/C/A/S/N/X/H） | `window/app.py:1200` | Lumino 仅播放/编辑快捷键，无字母键切工具 |
| 19 | **Help 系统**（F1 可搜索 + 首次 tip + 演示 GIF） | `window/help.py`、`help_texts.py` | 无 |
| 20 | **History 面板**（命名撤销步骤列表） | `window/history.py` | 仅 Ctrl+Z/Y |
| 21 | **Numbers 数学表达式输入框**（`960*4`） | `files/mathexpr.py` | 未发现等价能力 |
| 22 | **形状分享为一行文本**（Export/Import） | `files/share.py` | 无 |
| 23 | **Text 工具参数**（字体/字号/行距/字距/阈值/grow） | `window/panel_text.py` | 仅 Normal / KeyRangeMerged 两模式 |
| 24 | **Snap 自定义吸附图片** | `window/snap_picker.py` | 有精度设置，无图片吸附 |

---

## 五、关键洞察（给排期用）

1. **引擎已就位，工具是缺口**：Lumino 的曲线→音符（`paths.py`）与填充几何（`custom.py`）已 1:1 移植，语义参考可**直接复用 Spiderweb 的算法**（甚至逐函数对照移植），实现成本主要在**交互层 + 参数面板**。
2. **`notes/` 24 模块 → 已搬 2 个**：`arc / bezier / claw / convert / engine / envelope / fonts / funnel / hzbass / joined / pattern / polygon / shrink / smooth / strum / text / tumour` 均未移植。建议按 P0 顺序：`tumour → claw → strum → pattern → funnel/hzbass → convert/joined → polygon/arc`。
3. ~~**Shape 工具本身还没对齐蜘蛛网语义**：`ui-editor/src/interaction/shape_tool.rs:7-9` 自述「音符生成**尚未**对齐曲线工具的蜘蛛网式处理」，仍是「按 snap 网格枚举格点、每格一个定长音符」——这是 P0 之前的**存量欠债**（形状与曲线两套音符生成语义不一致）。~~
   **✅ 已对齐（2026-10-04）**：形状工具的**描边**改走形状边界闭合折线
   （`editor-state/src/editor_state/shape_tool/geometry.rs` 新增 `shape_outline_path`：矩形/三角形取顶点 + 闭合点，
   圆按 64 段采样椭圆且末点显式复用首点），再交给曲线工具同一套逐音高行解析
   （`ui-editor/src/interaction/line_tool/paths.rs` `path_notes`）——每个音高行一条、两两无缝、
   长度由解析交点决定，**不再使用吸附精度**（闭合环从最左点重启、竖直段各占 1 tick 同口径）；
   **填充**腿仍是覆盖格点（`shape_cells(filled = true)` + 可选 `chop_span` 切分），
   与曲线工具「轮廓走 `path_notes` / 填充走区间切分」的结构一致。
   缺口只剩「填充的 Spam / Outline spam / Ends / 轮廓独立通道」等 P1 项（见 §四 P1-15）。
4. **Channels 是很多能力的公共底座**：Tumours/Claw/Strum/Formula 生成的重叠音符都需要分通道，`notes/engine.py` 的 As drawn/Single/Multi 建议与 P0 一起做，否则后续工具都会卡在「重叠被压平」。

---

## 附：证据文件索引

- Spiderweb：`window/app.py`（TOOLS）、`window/help_texts.py`（全部能力主题表）、`notes/*.py`（24 个生成模块）、`scripts/lang/en.json`（工具文案）
- Lumino：`core/core/src/types.rs`（`Tool`、`BrushConfig`）、`editor/ui-core/src/toolbar_event.rs`（`Event`/`ToolPanelItem`/`ShapeType`）、`editor/ui-editor/src/interaction/*`、`editor/editor-state/src/editor_state/*_tool.rs`、`editor/ui-editor/src/velocity/*`、`editor/ui/src/root/handlers/toolbar/tools/*`
- 移植来源注释：`ui-editor/src/interaction/line_tool/paths.rs:3`、`line_tool/fill/spans.rs:3`
