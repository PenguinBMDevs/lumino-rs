# gfx 着色器说明（滚动音符族）

> 这些内容原先以长注释写在 `onion_note.wgsl` 里。为守住「单文件 < 400 行」的硬线，
> 完整背景迁到本文，shader 内只保留**必要短契约**。代码语义未做任何改动。

## onion_note.wgsl — 统一全量音符渲染（洋葱皮 + 主音轨一体）

### 数据模型

GPU buffer 持有**所有轨全部音符**：`NoteInstance.border_width` 低 16 位 = 每侧边框
像素宽（生产值 1 ⇒ 每侧 2px），高 16 位 = `track_idx + 1`。「哪个轨是主音轨」由
`ViewState.current_track` uniform 低频更新（切轨**零重传**）。着色器内：

| 轨道身份 | 颜色 | 深度区 | 几何 |
|---|---|---|---|
| 主音轨（`track_enc == current_track`） | 强制主轨蓝 `MAIN_TRACK_COLOR` | 主音轨区（最前） | 正常 quad |
| 静音且非主音轨 | — | — | 退化为视口外同一点（零面积，不产生片元） |
| 其余（洋葱皮） | 实例固化的调色板色 | 洋葱皮区 | 正常 quad |

与旧 `note.wgsl` 的差异：**无预览哨兵分支**（预览音符走独立渲染器 `note.wgsl`）。
2026-08-07 起顶点输入从完整 `NoteInstance` 改为 `u32` 可见索引。

三条顶点入口（其余语义必须逐字一致，否则各路径画面不一致）：

| 入口 | 顶点输入 | 可见性判定 | 用途 |
|---|---|---|---|
| `vs_main` | 可见索引（cull 产出） | `cull.wgsl` compute pass | 默认路径 |
| `vs_direct` | `@builtin(instance_index)`（源索引） | 本入口自判（谓词与 `cull.wgsl` 逐字一致） | 省掉整趟 cull pass（PREF-005） |
| `vs_point` | `@builtin(instance_index)` | 本入口自判 | 亚像素档位点图元（PREF-004 P1） |

### 深度语义：区域化位空间映射（与 `note.wgsl` 同义，2026-09 加固）

根因（两半叠加）：

1. `cull.wgsl` 每个 workgroup 由线程 0 抢占式 `atomicAdd` 输出槽位，可见实例的
   **输出顺序**由 GPU 调度决定、帧间不稳定；
2. 管线为 `LessEqual` + `depth_write_enabled = true`（见 `constants.rs`），同深度
   「后画者胜」⇒ 赢家随可见缓冲顺序逐帧随机 ⇒ 重叠区描边闪烁。

解法是**与绘制顺序无关**的稳定深度：

1. **分层**：预览（`note.wgsl` 的 `0.0`，最前）`<` 主音轨区 `MAIN_DEPTH_REGION_BITS`
   `<` 洋葱皮区 `ONION_DEPTH_REGION_BITS` —— 主轨恒覆盖洋葱皮。
2. **区内全序**：深度 = 「区域起点 + **全局源索引**」的 f32 正位空间线性映射
   （`bitcast` 后位模式单调 ⇒ 索引严格等价于深度序）。全局索引 =
   `chunk_start + 本地可见索引`，跨 chunk 不重置；索引跨帧稳定（段内原位），
   洋葱皮区序 = 段表/轨道顺序，与绘制顺序无关。
3. **容量与饱和**：每区 `DEPTH_REGION_SLOTS` 槽（5.2 亿实例），覆盖项目 2.9 亿音符
   目标；超过槽数的极端索引饱和到区顶（确定性，但饱和段内仍可能平局——超出项目目标）。
4. **值域**：区域起点取最小正规格数 2^-126，洋葱区顶 0.25 < 远平面 1.0，
   全程正规格数、无 denormal、不越远平面。

CPU 孪生与精度契约见 `crates/editor/gfx/src/note_renderer/depth_tests/`。

### 边框判定：屏幕像素空间（2026-10-30，BORDER_PIXEL_SPACE）

旧实现按 UV 边距判边框：

```wgsl
let horiz_margin = 1.0 / half_width_pixels * border_px;   // 已废弃
is_border = uv.x < horiz_margin || uv.x > 1.0 - horiz_margin || …;
```

两个问题：

1. **等号归属不确定**：当填充边界恰好落在某个像素中心（fractional zoom / scroll
   下常见，例如 125% 缩放下 `sx + sw - 2 = 10.5` 正是像素 10 的中心），`uv == margin`
   的判定落在硬件重心插值的末位噪声上。实测同一行、`uv.y` 与纵向边距**逐位相同**
   的 f32（`0.1599999964237213`）的两个实例，一个判填充、一个判边框。
   而 CPU 光栅（`lumino-cpu-onion`）是确定性的「等号归填充」⇒ 出现 1px 边框/填充
   分歧（Step 6b 全像素对比抓到 125% 48 处、150% 36 处）。
2. **无法用固定 UV epsilon 修**：`eps_uv` 的等效像素宽度是 `eps * sw` —— 1e-4 UV 在
   1e6 px 宽音符上等于 100 px，会把整条边框吃光。

现实现：三条 quad 入口把矩形左上角 `rect_min`（屏幕像素）与尺寸 `screen_size` 以
**flat** varying 透传；FS 用 `@builtin(position).xy`（片元中心 = 精确 `x + 0.5`）做
纯像素域比较：

```wgsl
let border_px = f32(input.border_width & 0xFFFFu);
let side_px = border_px * 2.0;
let local = input.position.xy - input.rect_min;
is_border = local.x < side_px
         || local.x > input.screen_size.x - side_px
         || local.y < side_px
         || local.y > input.screen_size.y - side_px;
```

等号（`local == side_px` / `local == size - side_px`）归**填充**，与 CPU
`lumino-cpu-onion::notes::{fill_start, fill_end}` 同义。判定里没有任何插值量
⇒ 与音符宽度无关、没有宽度相关的 epsilon、退化几何下也确定。

回归证据：`crates/editor/gfx/tests/cpu_gpu_parity`（100%/125%/150% 含退化等号几何、
125%+0.3px 半像素滚动、1e6px 超宽音符）—— 全部逐像素严格 0 差异。

### 亚像素档位：点图元直绘入口（PREF-004 P1，2026-10-01）

背景（真机 RTX 2060 / 1920×1080 分段实测）：缩小全景时每帧成本 =
cull `1.39 ms` + draw `11.97 ms`（16M 音符全部可见）。瓶颈不是 cull 的读取带宽，
而是**图元装配 + 小三角形光栅化**（每个音符 quad 只有零点几像素宽，仍要付
4 顶点 / 2 三角形的固定成本）。点图元把「4 顶点 2 三角形」降为「1 顶点 1 点」，
并顺带省掉整趟 cull pass（可见性判定移入本入口），是这一档位唯一能显著压低 `T` 的杠杆。

与 `vs_main` 的差异（其余语义必须逐字一致）：

1. 顶点输入改为 `@builtin(instance_index)` 直接索引源缓冲——点直绘不产出可见索引
   列表（无 cull），渲染侧因此不设顶点缓冲；
2. 落点取音符**长度/键高的中点**（与 quad 的视觉中心一致）；
3. 视口相交 / 静音轨 / 长度 ≤ 0 判定全部在本入口完成，不可见者输出视口外点；
4. 深度公式不变：`global_index = chunk_start + instance_index`（= 源索引），与 quad
   路径的 `chunk_start + 可见索引` 同值 ⇒ 两条路径深度逐位一致，重叠裁决语义不回退；
5. 片元不画描边——亚像素宽度下描边没有物理意义（quad 路径在此时每侧 2px 的描边
   会吃掉整个矩形，整块都变成描边色，反而更失真）。

### VS cull 直绘入口（PREF-005，2026-10-01）

背景：`cull.wgsl` 的可见索引由 workgroup 间抢占式 `atomicAdd` 写入，输出顺序由
GPU 调度决定 ⇒ 重叠片元按**随机深度序**到达，`LessEqual + depth_write` 下无处早拒，
每个片元全额付 FS + blend。实测（RTX 2060 / 1920×1080）：panorama 16M 全可见 =
compute cull `1.60 ms` + draw `13.44 ms`。

本入口把可见性判定搬进 VS，于是：

1. 不再需要 compute cull pass（省掉全量读 + 可见索引写 + indirect）；
2. `instance_index` 即**源索引升序** = `region_depth` 的近→远序 ⇒ 重叠片元后到者被
   early-Z 直接拒绝，FS 执行量降到 ≈ 屏幕像素量级；
3. 配合不透明管线（无 blend ROP），early-Z 不再被混合路径拖累。

与 `vs_main` 的差异（其余语义必须逐字一致）：

1. 顶点输入改为 `@builtin(instance_index)`（无顶点缓冲）；
2. 视口相交判定在本入口完成（谓词与 `cull.wgsl` 逐字一致），不可见 / 静音 /
   长度 ≤ 0 一律退化为视口外零面积点（净效果：不产生任何片元）。

**硬约束**：不得在此引入 LOD 剔除（「1px 以下音符仍绘制」，见 `cull.wgsl` 注释）。

## 相关文件

- `onion_note_vertical.wgsl`：纵向转置版（瀑布流/纵向卷帘）。**仍使用 UV 边距判边框**
  ——CPU 洋葱皮后端下纵向入口被禁用，故未与横向一起改；这是一处记录在案的不一致。
- `note.wgsl`：主轨/预览层，含预览哨兵分支（`border_width == 0xFFFF_FFFF` → 70% alpha、
  无边框）。
- `cull.wgsl` / `cull_vertical.wgsl`：可见性裁剪，输出可见索引 + indirect 参数。
