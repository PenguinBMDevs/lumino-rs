# 洋葱皮 GPU 常驻 vs Andromeda 可见性驱动预算方案 · 对比调研

> 日期：2026-10-26　范围：`../andromedacpp`（C++/OpenGL）洋葱皮渲染 vs `lumino` 洋葱皮 GPU 常驻路径
> 方法：源码实证 + 仓库内既有实测数据 + 量级推演（本轮**未新增 GPU 实测**，实测口径见 §6）
> 一句话：Andromeda 省显存的根因不是"用 CPU 画"，而是**常驻只是缓存、预算决定上界、可见性决定数据流**；
> Lumino 的对应弱点是"数据流按工程结构组织，而不是按可见性组织"——但它**不能整体照搬**，因为共享权威面与
> 用户的"亚像素音符也必须画"硬约束在 Andromeda 里都不存在。

---

## 0. 结论先行

| 维度 | Andromeda | Lumino 现状 |
|------|-----------|-------------|
| GPU 常驻语义 | **有界缓存**（LRU + 预算），未命中不影响正确性 | **不变量**：全工程全轨全音符常驻，且是多个渲染器的唯一权威 |
| 显存上界 | 预算决定：`clamp(空闲VRAM/2, 256MB, 6GB)` | 工程规模决定，无上界（`MAX_CAPACITY = usize::MAX`） |
| 单音符字节 | 12B（texel buffer，一音符一 texel） | 16B 实例 + 4B 可见索引 ≈ 20B（增长期更多） |
| 上传量 | O(可见)：8MB/帧预算 + 512K 音符分块 + 后台预热线程 | O(工程)：全量会话一次推完（2.9 亿 ≈ 4.6GB）；整轨 `TrackDelta` 无分帧 |
| 每帧剔除 | CPU `O(log n)` 游标 + `O(可见)` 打包 | GPU compute，每帧 `O(全部常驻)`（仓库实测 16M ≈ 1.60ms） |
| 超限行为 | 逐轨回退 CPU 直绘，画面正确 | 无回退：扩容失败只 `error!` 丢弃本次上传 |

**判决**：Andromeda 的方案**不能整体移植**（三条硬阻断见 §4.2），但它的五条原则里有三条与仓库既有
PREF-004 的 P1 方案**独立撞车**（可见区间 cull、去掉 VS 二次读、图元降级），说明方向正确；
本调研把它们收敛到同一条底层逻辑，并补两条 Lumino 尚未考虑的（静音轨常驻、缩容不归还）。
落地顺序建议：**T1 静默收益 → T1.5 紧凑实例 + 可见区间 cull → T2 窗口化（有前置条件，暂不推荐先做）**。

---

## 1. Andromeda 实现事实（源码级）

### 1.1 数据结构与预算

| 参数 | 值 | 证据 |
|------|-----|------|
| 单音符字节 | 12B（`static_assert(sizeof(midi::Note) == 12)`，一个音符 = 一个 `RGB32UI` texel） | `note_gpu_cache.h:20` |
| slab 粒度 | 64MB | `note_gpu_cache.h:162` |
| 全局预算 | `clamp(空闲VRAM/2, 256MB, 6GB)`，查询不到回退 1GB | `note_gpu_cache.cpp:61-77` |
| 单轨上限 | `min(预算/4, 192MB)`，超限不缓存 | `note_gpu_cache.h:152-154` |
| 每帧上传预算 | 8MB / 64 entry；单块 512K 音符；`glMapBufferRange` 无同步映射 | `note_gpu_cache.cpp:92-120, 354-387` |
| 回收延迟 | 释放区间等 2 帧才可复用（无 fence 的 GL 3.3 保险） | `note_gpu_cache.h:166` |
| CPU 直绘缓冲 | 8192 实例 × 24B，`orphan_and_set` 流式写 | `piano_roll.h:29`、`buffers.h:61-66` |

### 1.2 每帧决策链（`piano_roll.cpp`）

1. 选洋葱皮轨集合：`NoOnion / ViewPrevious / ViewNext / ViewAll`，当前轨单独处理（`piano_roll.cpp:308-346`）。
2. 当前轨：先走常驻，失败则 CPU 打包（`:704-827`）。
3. 洋葱皮轨由远及近：`update_cull_for_track` 得到 tick 窗口 `[first, end)`（`:830-862`），
   再 `draw_track_resident(...)`——**命中就 `glDrawArrays` 区间偏移，未命中返回 false**（`:677-702, 869-881`），
   随后逐音符 CPU 打包（`:884-953`）。
4. 深度区域代替排序：当前轨 0.02，洋葱皮 `0.05 + 0.9*(1 - (slot+1)/(slots+1))`，`LEQUAL`（`:419, 864-866`）。

### 1.3 缓存失败的三条通道

`ensure()` 返回 `nullptr` 的场景：预算/分配失败、本帧上传预算耗尽、单轨超上限（`note_gpu_cache.cpp:259-352`）。
关键设计：`evict_until_fits` **拒绝淘汰本帧已使用的 entry**，宁可 `budget_exhausted_` 后续帧不再尝试
（`:226-251`）——用"冻结工作集 + 回退"换掉"抖动淘汰"。

### 1.4 三层剔除，每层都有"不值得就不做"阈值

| 层 | 机制 | 阈值 |
|----|------|------|
| 区间 | 单调游标 + `partition_point`（按 `start` 二分），只处理视口 tick 窗口 | 时间倒退时回走/从头扫（`note_cull_helper.cpp:52-91`） |
| 遮挡 | 同 `key+channel` 槽位，被后音符完全覆盖即丢弃；后台低优先级线程构建 | 保留率 > 75% 标记 `NotWorthIt`（`note_occlusion.h:67-69`） |
| 屏幕覆盖 | 256 行（琴键）× 列/64 位图，已覆盖区不再提交（画家序不透明） | `worth_running` 门槛 + 索引上传需省 ≥512（`screen_coverage.h:24,49-51`、`piano_roll.cpp:570-584`） |

### 1.5 数据视图额外两招

- **密度上限**：二分出每轨可画上限 `c`，超出按 `stride` 抽稀，总量 ≤ `列数 × 4096`（`data_view.cpp:477-521`、`data_view.h:91`）。
- **静态视图 FBO 缓存**：视图 key 不变时整层合成进 FBO 直接 blit（`data_view.cpp:537-646`）。

---

## 2. 为什么"又快又省"

**省显存**：上界 = 预算，不是工程规模。未命中时数据仍在 CPU（`std::vector<Note>` 是权威），
所以"装不下"只影响性能、不影响正确性——这条性质是预算可以远小于工程的前提。

**快**：① 上传/剔除/绘制全部 ≈ 视口相关；② 上传预算化（8MB/帧 + 后台共享上下文预热，`glFinish` 只在批次边界）；
③ 12B/texel 紧凑表示，无 per-note 矩阵；④ 剔除后只提交可见索引；⑤ 滚动时游标增量前进；⑥ 深度区域免排序。

**它的代价（不吹）**：

- 时间倒退（拖滚动条）退化到一次性线性扫描（`note_cull_helper.cpp:54-71`）。
- 全 `ViewAll` + 超预算时工作集冻结，大量轨道长期走 CPU 直绘——CPU 打包 + 每帧 24B/音符 PCIe 流量，
  这正是它需要密度上限与覆盖剔除兜底的原因。**Andromeda 在"全景"下也退化，它靠的是密度上限。**

---

## 3. Lumino 现状测绘

### 3.1 事实

| 项 | 现状 | 证据 |
|----|------|------|
| 实例布局 | `NoteInstance` 16B | `crates/editor/gfx/src/note_renderer/types.rs:22-31` |
| 常驻语义 | "GPU buffer 常驻所有轨全部音符"（含当前轨与静音轨） | `.../runner/note_update.rs:51-60`、`onion_skin/stream.rs:15-19, 82-107` |
| 共享权威面 | 走带、瀑布流活跃键、miditrail 导出都直接绑这个缓冲 | `docs/2026-10-17-数据源冗余审计.md` §12.3；`arrangement_renderer/init.rs:91-93` |
| 预留 | `Reserve { total }` 消除 2× 倍增余量（2.9 亿：8.6GB → 4.6GB） | `stream.rs:66-80`、`run/onion_stream.rs:64-70` |
| 增量 | 主轨事件级（段内索引 = document 索引）；洋葱皮轨整轨 `TrackDelta` | `onion_segments.rs:1-26`、`stream.rs:181-204` |
| 可见索引 | u32/实例，2× 增长 + 1M 额外上限 | `note_renderer/buffer.rs:6-13, 36-61` |
| 容量回收 | 实例/可见缓冲只增不减 | `gpu_note_buffer/internal.rs:46-58`、`buffer.rs:36-61` |
| 显存预算 | 无（"用户硬约束：不得限制 GPU 内存使用"） | `gpu_note_buffer/ops/upload.rs:14-18` |
| 每帧剔除 | 每帧全 chunk dispatch，一音符一线程 | `note_renderer/prepare.rs:262-339`、`render_pass.rs:110-152` |
| 已有的直绘分支 | 亚像素 / vs_cull 模式跳过 cull，VS 自判可见（闸门：可见占比 > 90% 才占优） | `render_pass.rs:122-144`、`note_renderer/prepare.rs:262-279` |
| 大缓冲 | 2GB storage binding → chunk 绑定组 | `note_renderer/chunk.rs:1-40` |

### 3.2 定量（16B 实例 + 4B 可见索引）

| 音符总量 | 实例 | 可见索引 | 合计 | 每帧 cull 需读 |
|---------:|-----:|---------:|-----:|---------------:|
| 1M | 16MB | 4MB | 20MB | 16MB |
| 10M | 160MB | 40MB | 200MB | 160MB（60fps ≈ 9.6GB/s） |
| 50M | 800MB | 200MB | 1.0GB | 800MB（≈48GB/s） |
| 290M（代码注释口径） | 4.64GB | 1.16GB | **5.8GB** | 4.64GB（≈278GB/s@60fps） |
| 600M（代码注释口径） | 9.6GB | 2.4GB | **12GB** | 9.6GB |

**仓库实测锚点**（`render_pass.rs:122-137`）：16M 全景时 `cull ≈ 1.60ms`、`draw ≈ 13.44ms`；
cull 约 0.10ns/实例、直绘 VS 约 0.65ns/实例；1.5 亿全景单趟读 ≈ 2.4GB ≈ 6ms @400GB/s
（`docs/2026-10-01-PREF-004-滚动条拖拽全程卡顿调研与修复方案.md`）。
按 0.10ns/实例外推：50M ≈ 5ms、290M ≈ 29ms/帧——**cull 本身就吃掉 60fps 的全部预算**。

### 3.3 两种视口状态，成本结构完全不同

| 状态 | 可见占比 | 主导成本 | Andromeda 的做法 | 对本方案的意义 |
|------|---------|---------|-----------------|---------------|
| 放大编辑（常态） | <5% | cull 全量读 + 绘制可见 | CPU 只打包可见 | **窗口化收益最大** |
| 缩小全景 | ≈100% | 每帧 O(N) 的读 + 光栅 | 密度上限抽稀 + 覆盖剔除 | **窗口化无收益**（全可见），且与"亚像素必画"硬约束冲突 |

这条分界决定了：**不能承诺"窗口化后全景也变快"**。全景下 Lumino 按用户约束就是要画全量，
只能靠点图元/紧凑布局省带宽，不能靠少画。

### 3.4 结构性问题清单（按严重度）

1. **P0 每帧 cull O(常驻)**：与视口无关的固定税，2.9 亿时 ≈29ms/帧。
2. **P0 显存无上界**：2.9 亿 = 5.8GB，低显存设备 OOM 或落共享内存。
3. **P1 只增不减**：大工程切小工程后显存不归还（等渲染器重建）。
4. **P1 整轨 `TrackDelta` 无分帧**：编辑 3000 万音符的洋葱皮轨 = 单次 480MB 上传。
5. **P1 可见索引容量 2× 增长**：跨幂次最多浪费一倍。
6. **P2 静音轨是否必须常驻未确定**（见 §5 T1-1）。
7. **P2 全量会话一次推完**：中途失败只能 `force_full` 整盘重来。

---

## 4. 可移植性判断

### 4.1 一一对应

| Andromeda 机制 | Lumino 落点 | 判定 |
|----------------|-------------|------|
| slab + 空闲链表 + LRU | storage buffer + 固定块 + 空闲链表 | ✅ 可做（但有权威面问题，见 4.2） |
| 12B texel 缓存 | 8–12B 紧凑实例布局 | ✅ **性价比最高** |
| 8MB/帧上传预算 + 后台预热 | `queue.write_buffer` 异步；host 侧分帧节流 | ✅ |
| CPU 剔除 O(log n) | 文档每轨按 `start_tick` 有序，可二分/游标 | ✅（长音符需 `max_length` 回看） |
| 可见区间 cull | 复用已有 `GlobalBucketIndex.sort_index` 的有序 key 区间 | ✅ 已在 PREF-004 P1 立项 |
| 去掉二次读 | VS 不再读 `all_instances`，cull 直接产出紧凑实例 | ✅ 已在 PREF-004 P1 立项 |
| CPU 直绘回退 | 新增"实例作顶点缓冲"直绘管线（`prepare_direct` 已有雏形） | ⚠️ 中等工作量 |
| 密度上限 | **与"亚像素必画"硬约束冲突** | ❌ 不做 |
| 遮挡剔除 | 仅洋葱皮轨启用（当前轨禁用，避免选择歧义） | ⚠️ 可选 |

### 4.2 三条硬阻断（为什么不能整体照搬）

1. **共享权威面**：该常驻缓冲同时服务钢琴卷帘、走带、瀑布流活跃键、miditrail 导出
   （`data_source 审计` §12.3、`arrangement_renderer/init.rs:91-93`）。窗口化 = 把"唯一权威"降级为
   "某个视图的缓存"，要么分裂成多份缓冲（显存翻倍），要么让走带/瀑布流也窗口化（语义变更）。
   审计已明确：换源/改语义必须由像素等价 harness 在真实夹具上验收。
2. **硬约束冲突**：`cull.wgsl:81-82` 注释写明"不得 LOD 剔除，1px 以下音符仍需绘制"。Andromeda 的全景可用性
   一半靠密度上限——这条路在 Lumino 被封死。
3. **无空闲显存查询**：wgpu 没有 `NVX/ATI` 那类查询，预算只能配置/启发式（可复用
   `hires_gpu_mem_limit_mb` 的先例，`crates/core/core/src/storage/config.rs:169`）。

另外两点是工程现实，不是能力问题：
- `wgpu` 无 `glMapBufferRange(...UNSYNCHRONIZED)` 的无同步映射，但 `write_buffer` 本身异步，不构成阻碍。
- "不得限制 GPU 内存"的硬约束，**前提是超限会丢数据**；一旦有回退路径，预算限制的是缓存而非能力——
  这是后续要说服产品侧的关键论点，现在还不成立（因为还没有回退路径）。

---

## 5. 方案（按风险递增）

### T0 度量先行（0.5–1 天，必须先做）

- 计数器：常驻字节（实例/可见/段表）、全量会话耗时与字节、`TrackDelta` 字节、每帧 cull GPU 时长
  （`puffin` 已有 scope）、可见实例数、帧时 P50/P95。
- 基准夹具：① 单轨 3000 万音符黑乐谱 ② 200 轨 × 10 万音符 ③ 混合；各测"放大编辑 / 匀速滚动 / 全景"三态。
- 产出：工程规模 → 帧时/显存/上传 的曲线，作为 T1/T1.5 的验收基线。

### T1 静默收益（1–2 天，不改渲染语义）

1. **静音轨不常驻**——**前置确认**：走带/瀑布流是否显示静音轨。钢琴卷帘侧静音轨被 `ViewState`
   隐藏却照样上传（`stream.rs:15-19`）。若走带也不显示它们，则可跳过上传，省下静音轨占比；
   若走带显示，本条作废（需要"卷帘单独一份"才能做，不值得）。
2. **`TrackDelta` 分帧**：复用 `MAX_UPDATE_SEGMENT` 的分块思路，单条消息 ≤128MB，其余排队到后续帧；
   通道满降级机制（REND-019 PR-A′）可直接复用。
3. **缩容释放**：工程切小/关闭洋葱皮时重建渲染器或把容量收敛回 `max(实例数, 下限)`。
4. **可见索引容量收紧**：增长目标改 `required + min(required/4, 1M)`，削掉跨幂次的翻倍浪费。

### T1.5 紧凑实例 + 可见区间 cull（3–5 天，收益最大且不动权威面）

1. **实例 16B → 8B**（`start u32 + length u24 + key u8 + track u8`，tick/长度上限 16.7M，
   对现有 `total_ticks: u32` 工程需做一次上限校验与拒绝路径）。收益：显存减半、cull 带宽减半、
   VS 读量减半——全景场景同样有效，**不触碰任何权威面语义**。若追求零风险可先落 12B
   （`start u32 + len u32 + key u8 + track u8 + pad`，省 25%）。
2. **可见区间 cull**：`GlobalBucketIndex` 已产出 `sort_index` + `key_offsets`（key 主序），
   可见 key 区间在有序数组中是一段连续区间，cull 只 dispatch 该段（PREF-004 P1 已立项）。
3. **去掉 VS 二次读**：cull 直接产出渲染所需紧凑实例，VS 不再读 `all_instances`（同上）。

> 这三条与 Andromeda 的"紧凑表示 + 可见集裁剪"同源，但**不引入缓存/回退/预算**，因此不需要像素等价
> harness 之外的额外验收，风险等级远低于 T2。

### T2 窗口化常驻 + 预算 + 回退（5–10 天，**有条件再上**）

**前置条件（缺一不可）**：
① 像素等价 harness 在真实黑乐谱夹具上就绪（审计 §12.3 的既有结论）；
② 走带/瀑布流/导出各自明确数据来源（共享 vs 独立缓冲的显存账要算清，可能翻倍）；
③ 产品侧接受"洋葱皮缓存有预算上限"。

**设计**：`(track, block)` 为单位（`block = start_tick / BLOCK_TICKS`，块内上限 64K 音符 = 1MB），
固定 slab + 空闲链表 + `last_used_frame`；每帧由 host 用相机算出 `want = viewport × overscan`
（再回看 `track_max_length` 覆盖跨窗长音符），按 `BUDGET_PER_FRAME` 上传，超出 `keep_window`（3× 视口）的块延迟 3 帧回收；
缺块用直绘管线（`prepare_direct` 雏形）兜底。**仅对非当前轨窗口化**，当前轨保持整轨（段内索引契约不变）。

**收益边界（说清楚）**：放大编辑场景显存从 O(工程) 降到预算量级、cull 从 O(工程) 降到 O(视口)；
**全景场景无收益**（全部可见，本就要画全量），只能靠 T1.5 的紧凑布局。

### T3 可选

- 遮挡剔除（仅洋葱皮轨），借鉴 `KEEP_BELOW = 0.75` 的"不值得就不做"。
- 全景点图元（PREF-004 P2 已提，需像素级 A/B 确认外观）。

---

## 6. 验收与度量

| 指标 | 口径 | 目标 |
|------|------|------|
| 显存 | `memtrace` 实例 + 可见 + 段表 | 2.9 亿工程：T1.5 后 ≤ 2.9GB（现状 5.8GB） |
| cull | GPU timestamp / puffin | T1.5 后 ≈ 现状 50% |
| 帧时 P99 | 放大 / 匀速滚动 / 全景三态 | 不劣于现状，全景不回退 |
| 上传 | 至首帧可见耗时；`TrackDelta` 单帧字节 | T1 后单帧 ≤128MB |
| 正确性 | **像素等价**：改动前 vs 改动后静止帧逐像素比对 | 完全一致 |
| 单测 | 块划分边界（跨块长音符、零长音符）、紧凑布局编解码往返 | 必过 |

---

## 7. 风险与未解问题

1. **静音轨语义**（阻塞 T1-1）：走带/瀑布流是否显示静音轨，未在源码中找到明确依据，必须先确认。
2. **紧凑布局的 tick 上限**：8B 方案要求 `total_ticks < 2^24`；超限工程需要 12B 回退或版本化布局。
3. **共享权威面**（阻塞 T2）：见 §4.2，T2 之前必须先把"谁读哪份"的账算清。
4. **直绘回退一致性**：描边宽度、色彩、深度区域必须与常驻路径逐位一致，否则滚动闪烁。
5. **未实测**：§3.2 的 290M 推演基于仓库 16M 实测的线性外推，T0 必须补 100M+ 实测点。

---

## 8. 参考

- Andromeda：`src/app/rendering/note_gpu_cache.{h,cpp}`、`piano_roll.cpp:293-989`、`note_cull_helper.cpp`、
  `note_occlusion.{h,cpp}`、`screen_coverage.{h,cpp}`、`data_view.cpp:477-646`、`note_uploader.cpp`
- Lumino：`crates/editor/gfx/src/note_renderer/{types,buffer,prepare,chunk}.rs`、
  `shaders/{cull,note,onion_note}.wgsl`、`gpu_note_buffer/ops/upload.rs`、
  `render_thread/render_loop/{render_pass.rs, runner/onion_segments.rs}`、
  `crates/editor/ui/src/host/render/onion_skin.rs` + `onion_skin/stream.rs`
- 仓库既有结论：`docs/2026-10-01-PREF-004-滚动条拖拽全程卡顿调研与修复方案.md`（P1 三项）、
  `docs/2026-10-17-数据源冗余审计.md` §12.3（共享权威面与像素等价 harness 前置）
