# 2026-10-17 Spiderweb 图形工具缺口清单与移植规划

> 调研对象：`D:\LuminoDevelop\Source\Spiderweb`（Python/Tkinter，MIT，164 个 .py）
> 图形栈代码量（**含空行**，`Get-Content | .Count` 口径）：`scripts/notes/` **21,969 行**、
> `scripts/window/` **37,795 行**（其中与图形工具直接相关的面板/窗口约 18,000 行）、`scripts/roll/` **6,201 行**。
> 对比对象：`lumino-rs`（Rust，`crates/editor/*`、`crates/audio/note-core`）
> 结论口径：**「缺口」= Spiderweb 有、lumino 没有对应能力**；「部分覆盖」= 有但语义/参数显著更弱。
> 证据一律落到 `文件:行号` 或精确符号名；不带证据的条目不进清单。

---

## 0. 执行摘要

1. **Spiderweb 的「图形工具」不是 12 个画图工具，而是「参数化形状 + 有序修饰链 + 音符级生成器」的创作系统。**
   12 个绘制图元（`app.py:96-101` `TOOLS` / `DRAW_TOOLS`）只是入口；产能来自 **形状参数对象**
   （`engine.py` `clean()` 归一化：kind / pts / fill / gate / ends / tumour / glue / fx / sustain / mask /
   pattern / nested / tiles / areas / polygon / text / picture / gate_flip / vel_env / between / cut）
   和 **fx 步骤链**（`fx.py:1-52`：chop / claw / strum / path 按添加顺序重跑，外加 flip / turn / vel 步骤）。
   「绘制 → 烘焙成音符 → 丢弃矢量」这个范式，Spiderweb 从来不用。
2. **lumino 拿到了 Spiderweb 的「引擎」，但没有「工具集」，更没有「参数化形状层」。**
   已 1:1 移植：`ui-editor/src/interaction/line_tool/paths.rs` ← `notes/paths.py`（逐音高行跨越）；
   `line_tool/fill/spans.rs` ← `notes/custom.py`（`poly_edges`/`row_spans`/`merge_spans`，even-odd 填充）。
   `notes/` 下 46 个模块，lumino 只搬了 2 个算法。
3. **清单 46 条 = 45 项真实缺口 + 1 条已覆盖对照（G44 Domino 剪贴板）。**
   分布：P0 12 条（黑乐谱核心 + 公共底座）、P1 19 条（表现力/效率）、P2 15 条（工程体验）。
4. **瓶颈不是算法，是数据结构**：lumino 的 `DrawnShapeSource`
   （`editor-state/src/editor_state/drawn_shape.rs:40-58`）只有 `Shape{kind,rect,shift,filled,apex}` 与
   `Polyline{points}` 两种**几何**来源；`DrawnShape`（`:155`）只支持**平移**（`translate`/`translate_shape`），
   **没有参数、没有重生成、也不参与工程序列化**（`:16-19` 明写「注册表不参与文档序列化」）。
   在这层之上直接加 Tumours / Claw / Strum，只会得到「一次性烘焙的孤儿音符」——改一个参数就要重画，
   等于把 Spiderweb 最值钱的部分（可回改）丢掉。
5. **路线**：阶段 0 把「参数化形状 + 修饰链 + 通道」打进文档模型 → 阶段 1 上 8 个纯音符算子（收益/成本最高）
   → 阶段 2/3 补填充与互操作 → 阶段 4 体验 → 阶段 5 再评估 Hz bass。
   估算 **125～150 人日**（含测试与文档）；**MVP（S0+T1 主干+Arc/Polygon）约 41～45 人日**。

---

## 1. 对标口径

| 维度 | 纳入 | 不纳入 |
|---|---|---|
| 绘制图元 | 选择/切割/线/折线/手绘/曲线/圆弧/形状库/圆/多边形星形/漏斗/文字/Hz bass/图片 | — |
| 形状后处理 | 填充四态+末端+门控范围+描边、嵌套、壁纸、肿瘤、爪机、扫弦、切分、粘合、延音、门控翻转、遮罩、补间、沿路径复制、公式、对称、连接/合并/切割、变换编辑、颜色通道、力度 | 播放/导出/界面外观 |
| 工程体验 | 快捷键、形状库/抽屉、分享、历史面板、表达式输入、吸附图、帮助、自动保存 | 自动更新、安装包 |

对照基准：`docs/spiderweb-vs-lumino-音符绘制工具对比.md`（2026-10-04，commit `ed6b1866`）。
**该文档已部分过期**：Spiderweb 在 10-06～10-12 新增 `fx.py`（步骤链）、`sliced.py`、`merge.py`、`mask.py`、
`copypath.py`、`mould.py`，旧文档全部未覆盖；且旧文「Ends 8 种」有误（实为 **5 种** × Start 两态组合出 9 张图）。
本文以当前源码重新盘点。

---

## 2. lumino 现状基线（证据）

### 2.1 工具面

| 层 | 现状 | 证据 |
|---|---|---|
| 工具枚举 | `Pointer / PointerYSelect / Pencil / Brush / Pen / Eraser / DrawEraser / Razor / ShapeSelect / Curve / Shape / Text`（`Pen`/`Razor` 占位） | `crates/core/core/src/types.rs:35` |
| 绘制悬浮条 | `Mouse / Curve / FillBucket / Brush / Shape / Text` | `crates/editor/ui-core/src/toolbar_event.rs:189` |
| 形状种类 | 仅 `Rectangle / Circle / Triangle` | `toolbar_event.rs:169`、`editor-state/shape_tool.rs:32` |
| 单键切工具 | **无**（键盘处理里没有 `Tool::*` 赋值） | `crates/editor/ui/src/host/event/keyboard.rs` 全查无 |
| 图形对象 | `ShapeSelect` 点选/框选/平移/删除已确认图形 | `ui-editor/src/interaction/drawn_shape.rs` |

### 2.2 音符生成内核（已对齐 Spiderweb）

| 能力 | lumino | 移植来源 |
|---|---|---|
| 逐音高行解析跨越、两两无缝连奏 | `ui-editor/src/interaction/line_tool/paths.rs` `path_notes` | `notes/paths.py` |
| 闭环内部填充（even-odd、逐音高行区间） | `line_tool/fill.rs` + `fill/{loops,region,spans}.rs` | `notes/custom.py` |
| 形状描边走曲线同源解析 | `editor-state/shape_tool/geometry.rs` `shape_outline_path` | 2026-10-04 对齐 |

### 2.3 填充

- 内部填充 + **x 分音符切分**（`LineToolState::fill_division: Option<u32>`，`editor-state/line_tool.rs:133`；
  `fill/spans.rs::chop_span`）；
- 端点容差闭合（1 格内自动封口）、未闭合路径蔓延到视图边界（`fill.rs:1-15`）。
- **缺失**：`Empty / Fill / Spam / Outline spam` 四态；Spam 起始三态；Ends 五态；门控范围 Range；
  轮廓独立通道；Outline gate 加粗；重叠抵消开关；缺口自动接合口径；区域着色。

### 2.4 已确认图形对象（DrawnShape）

- 结构 `id / track / group / source / notes / deleted / moves / pending`（`drawn_shape.rs:155-182`）；
- 编辑 **只有平移**（`:81 translate`、`:590 translate_shape`），无缩放/单边/旋转/倾斜；
- 无再生成（`set_source` 仅用于覆盖镜像几何，`:342`）；
- **不参与文档序列化**（`:16-19`）。

### 2.5 其它相关现状

| 能力 | 现状 | 证据 |
|---|---|---|
| 通道 | `Note.channel: u8` 存在，**无自动分通道/重叠消解** | `audio/note-core/src/note.rs:5-16` |
| 力度 | 点拖拽 + 自动化曲线；无 Pencil / Formula / 形状力度包络 | `ui-editor/src/velocity/**` |
| 文字 | 仅字体族 + `Normal / KeyRangeMerged`；阈值/字号/行距/字距/grow 全为常量 | `editor-state/text_tool.rs:12`、`text_tool/rasterize.rs:10` |
| 图片转音符 | 有（区域框选 + 预览），无调色板拟合/混色/播放器外观 | `editor-state/image_to_midi.rs` |
| 自动保存 / 崩溃日志 | 未发现 | `crates/` 内 `autosave` 命中 0 |
| 历史面板 / 表达式输入 / 形状分享 / 形状库 | 均无 | `shape_history` / `mathexpr` / `share_shape` / `shape_library` 命中 0（`expr` 只命中宏元变量与音频 expression CC） |

---

## 3. Spiderweb 图形工具全景（证据）

### 3.1 绘制图元（12 + 选择 + 切割）

取自 `app.py:96-101`：`TOOLS=[("select","v"),("slice","k")]`；
`DRAW_TOOLS=[line/l, poly/p, free/f, curve/c, arc/a, custom/s, circle/o, polygon/q, funnel/n, text/x, hz/h, picture/i]`。
选择器状态与分组见 `window/tool_picker.py`（`FIRST="line"`、`GROUP=("custom","circle","polygon")`）。

| # | 工具 | 键 | 交互 | 后端 / 参数要点 |
|---|---|---|---|---|
| 1 | Select | V | 拖框（触及即选）/ Ctrl 加选 / 点选依次下一个 | 编辑（`pianoroll.py:874-936`） |
| 2 | Slice | K | 拖一条直线，松手即切；Shift 不吸附 | `slice.py` + `sliced.py` |
| 3 | Line | L | 拖拽或两点点击 | `paths.py`；`end_dot` |
| 4 | Polyline | P | 逐点点击或逐段拖拽；双击/右键/Enter 结束 | `paths.py`；`end_dot` |
| 5 | Freehand | F | 按住拖画；末点近起点自动闭合 | `smooth.py`；`Straighten 0-100` |
| 6 | Curve | C | 拖或两点，再拉锚点/手柄；中键加锚 | `bezier.py`；`sharp / sym / shape / pattern` |
| 7 | Arc | A | 三点（起/过/终）；记录 `k=sy/sx` | `arc.py`；`k`(beats per key) |
| 8 | Custom shape | S | 拖框/两角；Ctrl 保持比例 | `custom.py` + 形状库 |
| 9 | Circle | O | 拖框/两角；Ctrl 正圆 | `custom.py` + `polygon.py` |
| 10 | Polygon | Q | 同上；Ctrl 等边 | `polygon.py`：`{points,style(polygon/star/cross),inner,skip}`，`MAX_POINTS=200` |
| 11 | Funnel | N | 先画线再画墙；Ctrl 墙居中 | `funnel.py`：`fill(spam/long)、gate(0.0625)、wall(in/out)、range` |
| 12 | Text | X | 点击即打字（Enter 换行） | `text.py`：`font,size,unit,weight,italic,tracking,leading,align,threshold(50),grow` |
| 13 | Hz bass | H | 点空处新建 tone 形状 | `hzbass.py`（详见 §3.6） |
| 14 | Picture | I | 图像窗 → 拖入/Place at play line | `picture.py`：24 项参数（详见 §3.5） |

共用交互（`help/shortcuts.txt`）：Shift 不吸附、Ctrl 完美比例、Esc 取消半成品、Enter/双击/右键结束、
滚轮/Shift+滚轮/Ctrl+滚轮/Alt+滚轮 = 滚动/横向/缩放/只缩时间/只缩音高。

### 3.2 形状参数化模型（核心）

`clean()`（`engine.py:135-312`）把形状归一化成**可持久、可回改**的参数字典；`KINDS`（`engine.py:53`）：
`line / poly / free / curve / arc / custom / funnel`（text=带 `text` 的 custom，polygon=带 `polygon` 的 custom，
picture=带 `picture` 的 custom，hz=带 `hz` 的 custom）。

**fx 步骤链**（`fx.py:1-52`）：`sh["fx"]` = 有序步骤列表
`{"tool":"chop"|"claw"|"strum"|"path","cfg":…,"off"?:true}` + `flip`(axis) / `turn`(deg,r) / `vel`(pts) 步骤。
**每一步都在「改完之后的新音符」上重跑**：所以移动、缩放、旋转、改填充后，效果全部自动跟随；
同一工具可以加多页；翻转/旋转/力度会作为「步骤」夹在中间（`fx.py:7-17`）。

**渲染顺序**（`engine.py` `fx_notes`）：`形状→音符` →（图片特殊：不参与 glue/fx）→ `glue` → `fx` 页面按序
→ **`gate_flip` 早于 glue/pages** → `sustain` **永远最后**（`sustain.py` 顺序表）。
任何移植实现都必须把这个顺序写进单测，否则同一组参数会得到不同音符。

### 3.3 填充体系（`custom.py:33-63`）

| 项 | 取值 | 语义 |
|---|---|---|
| `FILLS` | `empty / fill / spam / outline_spam` | 只描边 / 每键每段一长音 / 门长切片 / 描边按门长切 |
| `ALIGNS`（Spam 起点） | `auto / aligned / centred` | 各行自左边缘 / 对齐 tick 0 网格 / 余量两端分摊 |
| `ENDS`（余数） | `round / keep / drop / min / stretch` | ≥半门取整门否则丢 / 保留短音 / 丢弃（短段仍留一音）/ 同 drop 但不短于 1/4 门 / 整段伸缩精确吻合 |
| `APART_FILLS` | `fill / spam / nested / tiles` | 这些填充支持 Normal / Outline（描边独立通道） |
| 其它 | `union`（取消 even-odd）、`borders`（色块间描边）、`edge/edge_mode`（Outline gate 加粗）、`range/range_kept` | `custom.py:48-52`，`convert.py:26` |
| 缺口接合 | `fill_plan`：1/64 beat + 1 key 内自动接合；几乎直的开口不填 | `custom.py:410-460`、`help/fill.txt:67-74` |
| Even band | `shrink.py` 距离场内缩（`SAMPLES=8, GRID=160, BLOCK=24`） | `help/fill.txt:87-89` |
| 门控范围 | `gaterange.py`：`sh["range"]={to,graph,dir,fit,rows,join,halves,follow}`，`DIRS=(time,keys,keys_down)` | 逐 tick 门长渐变 |

### 3.4 形状后处理（25 项）

| # | 能力 | 实现（行数） | 参数键（可照抄） |
|---|---|---|---|
| 1 | Inside fill + 末端 + Outline | `custom.py`(2943) | 见 §3.3 |
| 2 | Nested 嵌套填充 | `nested.py`(503) | `{way(scale/band/blend),steps,last,spacing,turn,colours,skip,at,mix,of{name,strokes},angle,morph}`，`MORPH_POINTS=200` |
| 3 | Tiles 壁纸填充 | `tiles.py`(203) | `{of,keys,length,gap_keys,gap,layout(grid/brick),flip(none/h/v/hv),by(diag/cols/rows/checker/one),colours,whole,at}`；上限 1e6 tiles / 8e6 notes |
| 4 | Tumours | `tumour.py`(544) | `{on,shape(triangle/square/circle/parabola),size,length,dist,side(alt/left/right/random),wrap(simple/wrap),start,end,ease,rot,slant,fit,seed,mirror,k,graphs}`；`MAX_TUMOURS=20000` |
| 5 | Claw machine | `claw.py`(138) | `{mode(time/notes/keys/chords/random),period(1/2/4/8/16),trash(k/n 含 None),dist(-100..100),stretch,short,keep,skip,pct,seed,shorten,cut}`；`BEND=ln50`、`SHORT=1/24` |
| 6 | Strum | `strum.py`(123) | `{time,tension,vel,vel_tension,end_time,end_tension,start,preserve,ahead,end,chop,alternate}`；`CAP=64` 拍 |
| 7 | Chop | `chop.py`(187) | 8 个命名节奏 `even/half/swing/offbeat/gallop/triplets/accent/stutter` + `{len,steps,pieces,abs,vel(%/fixed),amount}`；`MAX_PIECES=256` |
| 8 | Glue | `glue.py`(131) | `sh["glue"]=True` 或 `[[u0,v0,u1,v1],…]`；**执行最前** |
| 9 | Sustain | `sustain.py`(314) | `{gap?,most?,every?,from?,stops{at[],past?,off?}}`；Auto gap=最常见间隔+其一半；**执行最后** |
| 10 | Gate aware flip | `gateflip.py`(161) | `sh["gate_flip"]=True`；`NOT_DRAWN=(merge,notes,picture,hz)`；早于 glue/pages |
| 11 | Masks | `mask.py`(655) | `{size(keys,≤127),k,invert,own,show(inside/line),group,mode(add/subtract/intersect),colour}`；`SOLID=(fill,spam)`；**纯预览**，Apply 才产音符 |
| 12 | Add between | `between.py`(842) | `{id,role(first/last/step/key),steps,graph,rev,colours,smooth,push[x,y],at,sig,vel}`；`MOST_STEPS=500` |
| 13 | Copy along a path | `copypath.py`(811) | `{pts,mids,kind(straight/curve/arc),pattern,mode(every/count),anchor,half,k,speed(even/up/down/custom),curve,restart,colours,flip,on_top,range,size}` |
| 14 | Formula 形状/图案 | `pattern.py`(609) | **12 pattern preset**（wave/zigzag/bounce/square/saw/teeth/spikes/bumps/stairs/swell/fade/curls）+ **12 shape preset**（circle/spiral/flower/heart/figure8/slow/fast/scurve/reverse_s/quarter/exponential/logarithmic）；`FORMULA_KINDS=(curve,line,poly,arc)` |
| 15 | Custom edit 变换 | `custom.py` | 移动 / 角点缩放(Ctrl 比例) / 单边 / 15° 旋转 / 倾斜；`custom_hit → corner/side/turn/skew` |
| 16 | Straighten | `smooth.py`(348) | `smooth 0-100`，容差 `0.2*size*(level/100)^2.5`；`CORNER=40°`、`MAX_POLYGON=6`、`LOOP=0.1` |
| 17 | Symmetric halves | `pattern.py`(609)/`bezier.py`(676) | `SYMS=(mirror,turn)`；仅 Curve，joined 不支持 |
| 18 | Join / Split | `joined.py`(457) | `Ctrl+G` 合一 Curve（最近端串联，接触成角，不接触留 `gaps`）；`Split here` / `Ctrl+Shift+G` |
| 19 | Gate sensitive merge | `merge.py`(465) | `{ppq,keys,…}`；另一形状逐 key 行滑到相接；无对手行成 `Merge leftovers` |
| 20 | Slice / Mould | `slice.py`(405)+`mould.py`(140) | 自定义形状贯穿切两半；`sliced.whole/was/part/knife/vel/marks`；Mould=按音符逐行饼干模 |
| 21 | Live shape / Turn into live | `roll_live.py`(454)+`convert.py`(180) | `G` 画进同一 custom；`Ctrl+R` 多图元合一（`CAN_TURN=LINE_KINDS+custom`，原形状存 `sh["from"]`，`Ctrl+Shift+G` 还原） |
| 22 | Colours 变色 | `custom.py:53-63` | `CYCLES=(step,key,time)`；`cycle={by,n≤16,every,rows(True/"end"),shift[s,m]}`；`CYCLE_MAX=15` |
| 23 | Channels | `engine.py`(1304) | `raw/single/auto` + `SPLIT=(key,time)` + `use10`；`COLOURS=15, ALL_COLOURS=16` |
| 24 | Velocity pane | `window/velocity.py`(631)+`velocity_formula.py`(140)+`envelope.py`(73) | 四工具 `Linear/Curve/Pencil/Formula`；`sh["vel_env"]=[[u,velocity],…]`，缺省 `vel0→vel1` 斜坡 |
| 25 | 区域着色 | `areas.py`(326)+`faces.py`(239) | `sh["areas"]=[[u,v,colour]]`，0=空，1..15/16=色；`MAP_CELLS=1024`、`REACH=3` |

### 3.5 图片与文字

- **图片**（`picture.py` 717 行，函数级）：`load` → `pixels` → `resized/_shrink`（线性光平均，`MAX_SIDE=2400`）
  → `cells`（盒式降采样到 keys×steps）→ `adjust`（亮度/对比度/饱和度/锐化）→ `detail`（OKLab 邻域差异）
  → `fit_palette`（多图共享调色板、细节加权、锁定色）→ `quantise`（spread=Floyd–Steinberg / pattern=固定 4×4 / none）
  → `grid_notes` → `picture_shape` → `turned_notes`（旋转/倾斜后按 key 行重采样）。
  参数 24 项（`SUGGESTED`：keys/steps/view/colours/focus/blend/strength/keep/sharpen/brightness/contrast/saturation/
  empty/step/look/outline/shade/join/player_w/player_h/player_keys/by/fmt/use10）。
- **文字**（`text.py` 326 + `fonts.py` 342 + `fonts_ft.py` 365）：字体轮廓 → custom 形状；
  参数 `TEXT_DEFAULTS = {font,size,unit(font/rows),weight,italic,tracking,leading,align,threshold,grow}`；
  填充用 nonzero 规则，`threshold`=key 高度占比。

### 3.6 Hz bass（29 文件 / 16,444 行）

- **引擎**（`notes/` 9 文件 4,107 行）：`hzbass.py`(417) 数据模型 + `KeyGrid`；`hz_settings.py`(1071) 全部常量；
  `hz_grid.py`(993) key 级重复/响度；`hz_runs.py`(505) tone→repeats（Auto/fixed/mixed 门、滑音段）；
  `hz_modulate.py`(385) MOD 矩阵；`hz_lines.py`(255) 效果线；`hz_glide.py`(186) 滑音；`hz_arp.py`(162) 琶音；
  `hz_trace.py`(134) 曲线→音。
- **UI**（`window/` 20 文件 12,337 行）：旋钮 79 个、效果线 17 条、MOD 目标 78 个、MOD 源 6、波形 4、
  模式 5（fm/pulse/sync/growl/crush）、机架 5、层 ≤64、音色 8、琶音 5 型、音阶 9、和弦 7。
- **写进 MIDI 的**：`fx/loop/amount/from/fit/sustain/off/loud/length/stick`、`voice/mode/osc2/arp/mod/rack/trace`
  全部经 `KeyGrid → chop_grid → 音符`。**仅预览**：`hz_preview.py`、`hz_live.py`、`hz_sound.py`。
- ⚠️ BASS/BASSMIDI 为**非商用免费**授权，与 AGPL 冲突；音频侧不应照搬（lumino 有 xsynth / gpu-synth）。

### 3.7 工程化

形状库（`drawer.py:37 LIBRARY=<user>/shapes`、内置 `Circle/Square/Triangle`）、
History 面板（`history.py` 442 行，**最近 300 步**、可点击跳转/Undock）、
Numbers 表达式（`mathexpr.py` 145 行，任意数字框支持 `960*4`）、
Help（`help.py` 621 行 + `help_texts.py` 209 行，**79 个话题 / 8 分区** + 79 个 `help/*.txt`）、
Snap picker（`snap_picker.py` 226 行）、Domino 剪贴板（`domino_clip.py` 268 行）、
形状分享（`share.py` 129 行，前缀 `SPIDERWEB1:`，一行文本）、自动保存（`project.py` 818 行）、
错误日志（`errors.py` 230 行）。

---

## 4. 缺口清单

> 复杂度：简单（≤3 人日）/ 中等（4-8）/ 复杂（9-15）/ 极复杂（>15）。
> 「部分覆盖」= 有弱版，移植是改造而非新建。
> **「命中 0」口径**：在 `crates/` 内按该能力语义检索，无实现符号（注释、宏元变量、无关同名词不计）。

### 4.1 P0 —— 黑乐谱核心 + 公共底座（12 条）

| # | 缺口能力 | Spiderweb 证据 | lumino 现状 | 复杂度 | 依赖 |
|---|---|---|---|---|---|
| G01 | **参数化形状模型**（形状即文档对象，参数可回改可持久化） | `engine.py` `clean()`；`fx.py:1-52` | 只有几何来源、只可平移、不落盘（`drawn_shape.rs:40-58/155/16-19`） | 复杂 | — |
| G02 | **fx 修饰链**（有序、可关、可重跑） | `fx.py:1-52`；`engine.py` `fx_notes` | 无 | 复杂 | G01 |
| G03 | **通道分配底座**（raw/single/auto + Split + 通道 10） | `app.py:104-115`；`engine.py` | `Note.channel` 有字段无分配 | 中等 | G01 |
| G04 | **填充体系补齐**（四态 + Start 三态 + Ends 五态 + Range + Outline） | `custom.py:33-63`；`gaterange.py`；`shrink.py`；`areas.py` | 仅 fill + 分音符切分 | 极复杂 | G01 |
| G05 | **Tumours** | `tumour.py`；`help/tumours.txt` | 命中 0 | 复杂 | G02 |
| G06 | **Claw machine** | `claw.py`；`help/claw.txt` | 命中 0 | 中等 | G02 |
| G07 | **Strum** | `strum.py`；`help/strum.txt` | 命中 0 | 中等 | G02 |
| G08 | **Chop** | `chop.py`；`help/chop.txt` | 仅音符级 Razor / 全局量化 | 中等 | G02 |
| G09 | **Glue 形状级**（可框选、随形状自适配） | `glue.py` | 有音符级 `glue_selected_notes`，无形状级 | 简单 | G02 |
| G10 | **Sustain** | `sustain.py`；`help/sustain.txt` | 命中 0 | 中等 | G02 |
| G11 | **Gate-aware flip** | `gateflip.py` | 命中 0 | 简单 | G04,G02 |
| G12 | **Masks**（预览式 + Combine + Apply） | `mask.py`；`help/masks.txt` | 命中 0 | 复杂 | G01,G03 |

### 4.2 P1 —— 表现力 / 效率（19 条）

| # | 缺口能力 | Spiderweb 证据 | lumino 现状 | 复杂度 |
|---|---|---|---|---|
| G13 | Arc 三点圆弧（含整圆识别、`k` 比例） | `arc.py`(145) | 只能贝塞尔近似 | 简单 |
| G14 | Polygon / Star / Cross-star | `polygon.py`(106) | `ShapeKind` 仅 3 种 | 简单 |
| G15 | Funnel 漏斗 | `funnel.py`(919) | 命中 0 | 极复杂 |
| G16 | Live shape / Turn into live | `roll_live.py`、`convert.py` | 命中 0 | 复杂 |
| G17 | Nested 嵌套填充 | `nested.py`(503) | 无 | 复杂 |
| G18 | Tiles 壁纸填充 | `tiles.py`(203) | 无 | 中等 |
| G19 | Formula 形状/图案（自定义表达式+保存） | `pattern.py`(609)、`mathexpr.py` | 命中 0 | 复杂 |
| G20 | Symmetric halves | `help/symmetric.txt` | 命中 0 | 简单 |
| G21 | Join / Split here / Split separate | `joined.py`(457) | 只有音符级 Split/Glue | 中等 |
| G22 | Gate sensitive merge | `merge.py`(465) | 无 | 中等 |
| G23 | Copy along a path | `copypath.py`(811) | 无 | 极复杂 |
| G24 | Add between | `between.py`(842) | 无 | 复杂 |
| G25 | Custom edit（缩放/单边/15°旋转/倾斜） | `custom.py`；`help/custom_edit.txt` | 只平移 | 复杂 |
| G26 | Slice 工具（+ Mould 饼干模） | `slice.py`、`mould.py` | Razor 只切音符 | 中等 |
| G27 | Straighten（闭环认形状） | `smooth.py`(348) | 命中 0 | 中等 |
| G28 | 力度面板 Curve/Pencil/Formula + 形状包络 | `help/velocity.txt`、`envelope.py` | 仅点拖拽 + 自动化（部分覆盖） | 中等 |
| G29 | 图片工具增强（调色板/混色/落图/重生成） | `picture.py`、`help/picture.txt` | 有框选无调色板体系（部分覆盖） | 复杂 |
| G30 | 文字参数（字体/字号/行距/字距/阈值/grow） | `text.py`、`fonts*.py` | 阈值/字号为常量（部分覆盖） | 简单 |
| G31 | Colours 变色（step/key/time + 每行 + 对角线） | `custom.py:53-63`、`help/channels.txt` | 无 | 中等 |

### 4.3 P2 —— 工程体验（15 条，G44 为已覆盖对照）

| # | 缺口能力 | Spiderweb 证据 | lumino 现状 | 复杂度 |
|---|---|---|---|---|
| G32 | 工具单键（V/L/P/F/C/A/S/O/Q/N/X/H/I/K）+ Mask(M)/Live(G) | `app.py:96-101`、`help/shortcuts.txt` | 无 | 简单 |
| G33 | 形状库 + Drawer 自绘（含区域着色、角色/描边色） | `drawer.py`(1477)+`drawer_mouse.py`(912)+`drawer_areas.py`(273) | 无 | 复杂 |
| G34 | 形状一行文本分享（`SPIDERWEB1:`） | `share.py` | 无 | 简单 |
| G35 | History 面板（300 步、可跳转） | `history.py`(442) | 仅 Ctrl+Z/Y | 中等 |
| G36 | Numbers 表达式输入（`960*4`） | `mathexpr.py` | 无 | 简单 |
| G37 | 自定义吸附图片 | `snap_picker.py` | 有精度吸附 | 简单 |
| G38 | Help 系统（79 话题 / F1 可搜索 / 首次 tip） | `help.py`、`help_texts.py` | 无 | 中等 |
| G39 | 自动保存 + 崩溃日志 | `project.py`、`errors.py` | 未发现 | 简单 |
| G40 | 鼠标下音符试听（右拖） | README 快捷键表 | 需核实 | 简单 |
| G41 | Mould（饼干模切割） | `mould.py` | 无 | 中等 |
| G42 | 播放器外观预览 + 配色文本导出 | `help/picture.txt:22-34` | 无 | 中等 |
| G43 | Hz bass（音调→门控；合成器窗口另计） | `hzbass.py` + `hz_*` | 无 | 极复杂 |
| G44 | Domino 剪贴板双向 | `domino_clip.py` | **已覆盖**（`midi-model/src/clipboard/domino.rs`） | — |
| G45 | 256 键 / 31edo | `help/edo31.txt` | `key: u16` 已支持 >127，需核实 UI | 简单 |
| G46 | 再生性能闸门（百万音符） | `help/fill.txt:95` | 无形状再生概念 | 中等 |

> lumino 反向超出项（不计缺口）：图像转 MIDI 的区域框选工作流、视频/音频离线导出、协作与云、GPU 合成、Domino 剪贴板。

---

## 5. 架构决策（移植前必须定稿）

### D1 —— 形状是文档对象，不是音符的临时叠加

现状 `DrawnShapeRegistry` 明确不参与序列化（`drawn_shape.rs:16-19`），这是「临时叠加」定位的必然结果。
参数化形状必须**进文档**：
- 新增 `ShapeModel`（几何 + 样式 + `ops`），落在 `editor-state`；
- 进 `project` 序列化与迁移（旧工程无形状数据 → 空列表 + 版本字段）；
- 进协作同步（否则多人编辑时形状状态分叉）；
- 与历史分组绑定（一次参数提交 = 一个 undo group）。

**这是本规划最大的隐性成本**：不改这三处契约，G02 之后每条都会卡在「改完不能存/不能同步」。

### D2 —— 修饰链 = 有序步骤 + 全量重跑，不是「选项面板」

照抄 `fx.py` 语义比自创面板重要：每步作用在上一步输出上，可开关、可重复（同工具多页），
flip/turn/vel 也作为步骤夹在中间。好处是「改了形状效果自动跟随」；代价是必须做增量重算与预览隔离。

### D3 —— 预览 / 提交两阶段，复用现有 √/× 范式

lumino 的曲线/画刷/形状工具已是「绘制 → 待确认集合 → √ 一次写 document + 一次历史」
（`docs/2026-10-01-BRUSH-001`）。参数面板沿用同一范式：拖动旋钮只更新**预览音符**，OK 才提交，Esc 回滚。
这样再生开销不进历史、不进协作。

### D4 —— 通道分配必须先进底座

Tumours / Claw / Strum / Formula / Nested / Tiles 生成的都是**同 key 重叠音符**；
没有 G03，这些能力会互相压平。先实现 `raw` + `single`（规则明确、可测），`auto` 与 Split 口径随后。

### D5 —— 视图相关参数必须随形状存储

`k`（绘制时每 key 几拍）决定圆弧/肿瘤/遮罩带在屏幕上的「圆度」（`arc.py:1-8`、`tumour.py:19-21`）。
lumino 的 zoom 可变，**必须把 k 存进形状参数**，否则同一形状在不同缩放下走形——这是最容易漏的语义。

### D6 —— 不移植项（明确砍掉）

| 项 | 理由 |
|---|---|
| BASS / BASSMIDI 音频与合成器 UI | 非商用授权与 AGPL 冲突；复用 xsynth / gpu-synth |
| Hz bass 的全部效果编码（16,444 行中的 ≈12,000 行 UI） | 独立立项；先做「音调→门控」这 4,107 行引擎的对应部分 |
| 演示 GIF / 播放器外观像素级预览 | 资产不可搬运；改为文字帮助 + 简化预览 |
| Domino 剪贴板 | 已有 |

---

## 6. 移植路线图

> 落点约定（2026-10-17 修订）：**不新建 crate**。形状数据模型放 `lumino-core`
> （编辑器域数据类型同域先例：`Tool`、`BrushConfig`，且该 crate 已依赖 serde/bincode）；
> 音符级后处理放 `lumino-note-core`；文档级状态放 `editor-state`；交互与再生成放 `ui-editor`；
> 面板放 `ui/src/toolbar/`；窗口放 `crates/editor/dialog`。
> 新文件 ≤400 行（工作区红线）。每个任务独立 PR + `docs/` 实施记录。

### 阶段 0：底座（S0，13～16 人日；明细见 `docs/2026-10-17-S0-形状底座实施规划.md`）

| 任务 | 目标 | 落点 | 验收 | 估时 |
|---|---|---|---|---|
| S0-1 形状数据模型 | `kind` + 几何 + 样式 + `ops: Vec<ShapeOp>` + `k` | `lumino-core/src/shape.rs` | serde 往返；非法数据拒绝而非 panic | 4 |
| S0-2 工程部件 | 新部件 `.lmshapes`（缺失即空、非空才写） | `lumino-project`（folder + archive 双形态） | 往返 + 老工程保存字节不变 | 3 |
| S0-3 文档级形状存储 + 撤销日志 | 形状与音符同一 undo 单元（group 回放） | `editor-state` + `ui-editor` | 改参数 → undo → 形状与音符同时回退 | 5 |
| S0-4 通道底座 | `raw` / `single` + 消解规则 | `lumino-note-core/src/channels.rs` | 重叠/同 tick 合并用例 | 1.5 |
| S0-5 集成与兼容夹具 | 端到端 + 未知部件容错 + k 保真 | 各 crate `tests/` | 见 S0 文档 §5 | 1.5 |

> 与初版（14～18 人日）的差异：删掉 UI 预览面板与 `auto` 多通道分配（−4），
> 新增「形状撤销日志」这一原计划漏项（+2），新增老工程字节不变与未知部件容错夹具（+1.5）。

### 阶段 1：8 个纯音符算子（T1，25～28 人日）

| 任务 | 源文件 | 估时 | 备注 |
|---|---|---|---|
| T1-1 Tumours | `tumour.py` | 5 | 4 形状 × 4 side + Range；graph 二期 |
| T1-2 Claw | `claw.py` | 3 | time + 3 计数 + random |
| T1-3 Strum | `strum.py` | 3 | start/end/vel + chop/alternate |
| T1-4 Chop | `chop.py` | 3 | 8 命名节奏 + Quick chop；自绘 strip 二期 |
| T1-5 Glue | `glue.py` | 1.5 | 复用具名 glue + 自适配 |
| T1-6 Sustain | `sustain.py` | 3 | 分组 + gap 自动/手填 + stop |
| T1-7 Gate-flip | `gateflip.py` | 1.5 | 依赖 S0-5 |
| T1-8 Masks | `mask.py` | 6 | 预览层 + Combine + Apply |

### 阶段 2：填充补齐 + 生成器（T2，31～37 人日）

| 任务 | 源文件 | 估时 |
|---|---|---|
| T2-1 填充四态 + Start/Ends + Outline | `custom.py`、`help/fill.txt` | 6 |
| T2-2 门控范围 Range | `gaterange.py` | 3 |
| T2-3 Arc 工具 | `arc.py` | 2 |
| T2-4 Polygon/Star/Cross | `polygon.py` | 2 |
| T2-5 Formula（12+12 预设 + 表达式） | `pattern.py`、`mathexpr.py` | 5 |
| T2-6 Live shape / Turn into live | `roll_live.py`、`convert.py` | 4 |
| T2-7 Nested / Tiles | `nested.py`、`tiles.py` | 6 |
| T2-8 Funnel（先最简：两线一墙） | `funnel.py` | 7（完整口径另计） |

### 阶段 3：互操作与编辑（T3，40～48 人日）

| 任务 | 源文件 | 估时 |
|---|---|---|
| T3-1 Custom edit（缩放/单边/旋转/倾斜） | `custom.py` | 5 |
| T3-2 Join / Split here / Split separate | `joined.py` | 3 |
| T3-3 Gate sensitive merge | `merge.py` | 3 |
| T3-4 Symmetric halves | `bezier.py` | 1.5 |
| T3-5 Straighten | `smooth.py` | 3 |
| T3-6 Slice 工具（+ Mould） | `slice.py`、`mould.py` | 4 |
| T3-7 Add between | `between.py` | 6 |
| T3-8 Copy along a path | `copypath.py` | 7 |
| T3-9 Colours 变色 | `custom.py`、`channels.txt` | 2 |
| T3-10 力度面板三工具 + 包络 | `velocity.py`、`envelope.py` | 3.5 |
| T3-11 文字参数化 | `text.py`、`fonts*.py` | 2 |
| T3-12 图片调色板/混色/重采样 | `picture.py` | 4 |

### 阶段 4：体验（T4，14～18 人日）

快捷键 1 / 形状库+抽屉 7 / 分享 1.5 / History 面板 2.5 / 表达式输入 1.5 / 吸附图 1.5 / 帮助 3 / 自动保存 1.5。

### 阶段 5：Hz bass（另立项）

第一刀：`hzbass.py` + `hz_settings/hz_runs/hz_lines` 的「音调 → 非整数门控」部分（约 6 人日）。
合成器窗口与效果编码（≈12,000 行）需单独评估授权与架构，**不混进本轮**。

---

## 7. 工作量汇总

| 阶段 | 内容 | 人日 | 可并行度 |
|---|---|---|---|
| S0 | 底座 | 14～18 | 2 人（模型+存档 / 管线+通道） |
| T1 | 8 个算子 | 25～28 | 高（算子彼此独立，可 3 人并行） |
| T2 | 填充 + 生成器 | 31～37 | 中（Formula/Live/Nested 有依赖） |
| T3 | 互操作与编辑 | 40～48 | 中（变换/Join 是多项前置） |
| T4 | 体验 | 14～18 | 高 |
| T5 | Hz bass | 6 + 评估 | — |
| **合计** | | **125～150** | 含测试与文档 |

**MVP（41～45 人日）**：S0 全部 + T1 的 Tumours/Claw/Strum/Sustain/Chop/Masks + T2-3(Arc)/T2-4(Polygon)。
先交付这 10 项，直接补齐「黑乐谱纹理 + 形状生成器」主干。

**参考：Spiderweb 对应窗口规模**（说明 UI 成本，别只算算法）：
`path_window` 975、`panel_custom` 861、`pattern_dialog` 851、`between_window` 794、`sustain_window` 722、
`join_split` 643、`velocity` 631、`formula_host` 590、`graph_window` 581、`range_window` 565、`chop_window` 506、
`tumour_window` 430、`panel_mask` 345、`drawer` 1477。

---

## 8. 验收与度量

**功能验收**：每条缺口在实施记录里给「Spiderweb 行为 → lumino 行为」对照表 + ≥3 个语义用例
（边界：同 key 重叠、零长音符、tick 0 越界、百万音符上限）。

**语义回归**：为每个算子建立「Spiderweb 输出快照」夹具——把 Python 侧同一输入导出的
`tick/key/length/velocity/channel` 存成 JSON，Rust 侧断言全等（或文档化容差）。
顺序类能力（glue→pages→gateflip→sustain）必须单独有「顺序不变性」测试。

**性能门槛**：
- 拖动参数预览 P95 ≤ 16ms（单形状 ≤2 万音符），超出降级为轮廓预览并提示；
- 提交 ≤200ms/万音符；
- 100 形状 / 100 万音符工程：存档与打开劣化 ≤10%。

**质量门槛**：`cargo clippy` 零警告、`cargo fmt`、被改 crate 单测全绿、单文件 ≤400 行、无 `unwrap`。

---

## 9. 风险清单

| 风险 | 影响 | 缓解 |
|---|---|---|
| R1 形状进文档的契约改造 | 存档/协作/撤销三处同时改 | S0 单独成阶段，先跑通「无 ops 的形状」全链路 |
| R2 `k` 丢失 | 形状跨缩放走形 | D5 + 固定 k 的单测 |
| R3 再生性能 | 拖参数卡顿毁手感 | 预览隔离 + 预算 + 降级（BRUSH-001 范式） |
| R4 通道语义差异（track vs channel） | 多通道能力互相压平 | S0-5 先定规则 + 特征测试 |
| R5 语义细碎（Ends 5 × Start 3 × Outline 2 …） | 细节吞进度 | 常用 20% 先做，其余标为已知差距 |
| R6 UI 只有 GIF 无设计稿 | 实现走偏 | 面板优先、视觉后置、复用 SettingsPanel |
| R7 Hz bass 授权 | 法务风险 | D6：不碰 BASS |
| R8 一次性摊大饼 | 30+ 任务同时开 | 阶段门控：MVP → 评审 → 下一阶段 |

---

## 10. 附录：证据索引

**Spiderweb**
- 工具/通道常量：`scripts/window/app.py:96-115`（`TOOLS`/`DRAW_TOOLS`/`CHANNEL_CHOICES`/`SPLIT_CHOICES`）
- 形状模型与渲染：`scripts/notes/engine.py`（`clean` 135-312、`KINDS` 53、`fx_notes`、`render` 1304）
- 修饰链：`scripts/notes/fx.py:1-52`
- 填充：`custom.py:33-63`、`gaterange.py`、`areas.py`、`faces.py`、`shrink.py`
- 生成器：`tumour.py` `claw.py` `strum.py` `chop.py` `sliced.py` `glue.py` `sustain.py` `gateflip.py`
  `mask.py` `between.py` `copypath.py` `pattern.py` `nested.py` `tiles.py`
- 图元/编辑：`arc.py` `polygon.py` `funnel.py` `smooth.py` `bezier.py` `joined.py` `merge.py` `slice.py` `mould.py`
- 图片/文字：`picture.py` `text.py` `fonts.py` `fonts_ft.py`
- Hz：`hzbass.py` + `hz_settings/hz_grid/hz_runs/hz_modulate/hz_lines/hz_glide/hz_arp/hz_trace` + `window/hz_*`
- 工程化：`share.py` `mathexpr.py` `history.py` `help.py` `help_texts.py` `snap_picker.py` `project.py` `errors.py`
- 文案即规格：`scripts/lang/en/help/{fill,nested_tiles,tumours,claw,strum,chop,glue,sustain,gate_flip,masks,between,path,formulas,symmetric,join,merge,slice,straighten,custom_edit,live,turn_live,picture,channels,velocity,hz_bass}.txt`

**lumino**
- 工具面：`crates/core/core/src/types.rs:35`、`crates/editor/ui-core/src/toolbar_event.rs:169,189`
- 形状：`crates/editor/editor-state/src/editor_state/shape_tool.rs:32`、`.../shape_tool/geometry.rs`
- 曲线/填充内核：`crates/editor/ui-editor/src/interaction/line_tool/{paths,fill,fill/spans}.rs`
- 图形对象：`crates/editor/editor-state/src/editor_state/drawn_shape.rs:40-58,155,342`
- 音符：`crates/audio/note-core/src/note.rs:5-16`
- 文字：`crates/editor/editor-state/src/editor_state/text_tool.rs:12`、`ui-editor/src/interaction/text_tool/rasterize.rs:10`
- 力度：`crates/editor/ui-editor/src/velocity/**`
- 旧对比：`docs/spiderweb-vs-lumino-音符绘制工具对比.md`（2026-10-04，已被本文覆盖）
