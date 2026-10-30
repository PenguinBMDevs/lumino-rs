# LGS × Kestrel 架构对比与性能优化交接

> **日期**：2026-10-18
> **范围**：LGS（`crates/audio/gpu-synth`）与 kestrel-midi（GitHub `thtstickyboi/kestrel-midi`；本地副本 `E:\kestrel-midi`，main 分支）
> **用途**：为 REF-LGS-001…018 系列卡提供架构背景、性能靶点地图与"已知未知"清单，供接手者（人 / AI agent）快速进入上下文
> **证据标记**：✅=分析时逐行实读的代码；📌=来自 REF-LGS 卡或项目文档的引用；〔推断〕=基于代码的推理，未经实测

---

## 0. 阅读指引

建议顺序：§1（结论）→ §2（基准）→ §5（差距分解）→ §7（18 卡映射）→ §8（红线）→ §9（已知未知，**接手前必读**）→ §10（靶点地图）。

相关材料：
- Kestrel 本地副本 `E:\kestrel-midi`。注意：其源码内只留 `// [N]` 标记，**完整讲解在 `comments/` 同名 `.txt`**（评审时配对读）。
- 度量命令（全系列卡统一口径）：
  `LUMINO_PROFILE=1 cargo run --release -p lumino-gpu-synth --example profile_gpu -- <midi> <sf2> <frames> <block>`
- 夹具：`sustained2048.mid` / `sustained65536.mid` / `rich.sf2`（kestrel `gen-assets` 生成，可重建）。

---

## 1. 结论（TL;DR）

1. **LGS 与 Kestrel 的差距不是复杂度量级差**：两边对声部数 N 都是线性（渲染主项 Θ(N×sr) 是物理下限）。差距来自：
   - **两个结构性项**（对 N 的复杂度归属不同）：
     - 宿主状态往返：LGS 每块 O(活跃声部)（重建+上传+回读+合并）；Kestrel **稳态 O(1)**（状态设备自持，仅回读 64 B 摘要、只上传增量）；
     - 混合拓扑：LGS 每帧一线程串行扫全声部（并行度 = 块大小）；Kestrel = workgroup 归约 + partials 树（并行度与声部数解耦，读量 ≈ V/256）。
   - **四个常数维度**：每声部每块的字节数、访存模式（voice_out 两趟出入显存）、并行形状（+150% 段重放）、宿主参与度。
   - **一个容量耦合**：`voice_out = voices × block × 8 B` 上限 2 GiB − 64 KiB → 512 块只到 ~52.4 万声部、4096 块只到 ~6.55 万。
2. **量化**：每声部边际成本（斜率）LGS ≈ 61 µs（@512）/ 24 µs（@4096）/声部/音频秒 vs Kestrel ≈ 0.78 µs —— **差 30~78×**；固定开销两边同量级（LGS ~0.043 s、Kestrel ~0.058 s/音频秒，Kestrel 的截距≈其宿主单核天花板 ~17×）。〔推断：两点线性拟合，基于外部基准表〕
3. **18 卡做完的定位**：把差距从"设计缺陷级"（数量级差的来源）降到"工程完成度级"（数倍差）；中低声部区间有望接近甚至反超（收割 Kestrel 的固定开销弱点）；极限区间（10 万+持续声部）仍取决于 §6 残余项是否立项。

---

## 2. 基准数据（外部来源，未独立复测）

### 2.1 同机对比（RTX 2060 6GB + i7-9750H，48 kHz，Kestrel 1.3.0 vs LGS profile_gpu）

| 测试 | Kestrel | LGS | 差距 |
|---|---|---|---|
| 真实黑乐谱 100 s（峰值 8.9k vs 6.4k 声部） | 6.87 s = 14.55× | 27.24 s = 3.67× | 3.97× |
| 合成 2,048 持续音（~930 声部） | 17.02× | 10.03× | 1.70× |
| 合成 65,536 持续音（~29.4k 声部）@512 | 12.33× | 0.541× | **22.8×** |
| 同上 @4096 | 12.33× | 1.39× | **8.9×** |
| 合成 104 万持续音（峰值 470,612 声部） | 1.57×（0 抢占/0 丢弃） | 未测（外推 ~0.02×） | ~70× |

### 2.2 卡内基线（本机、profile_gpu、sustained65536）

- 块长四点：**512 = 0.541×｜1024 = 0.89×｜2048 = 1.24×｜4096 = 1.39×**（512 块单块预算 10.67 ms，实测 GPU ~24 ms，mix pass 为主）
- 纯 GPU 口径：**0.83 ns/声部/帧**（REF-LGS-013）；`sustained2048` = 10.03×；真实黑乐谱 100 s = 3.67×

---

## 3. Kestrel 架构（✅ 已验证）

### 3.1 渲染循环
每块一个 command buffer、五段：**steal → spawn → render → reduce → compact（sort 在 compact 段内）**。宿主与设备是"一块深"流水线（宿主备下一块，设备渲当前块）。✅ `gpu/mod.rs::submit()`

### 3.2 状态与数据面（与 LGS 的根本差异）

| 机制 | 实现 | 证据 |
|---|---|---|
| 状态自持 | voice 池 = 单 storage buffer 内 SoA，26 字段/槽（`f * capacity + i`），**宿主无 per-voice 镜像** | ✅ `common.wgsl` 字段表、`spawn/compact.render.wgsl` |
| 状态回读 | **每块仅 64 B**（16 个 u32 计数器：live/stolen/dropped/peak…） | ✅ `mod.rs` S_* 常量、`submit()` 拷贝 |
| 增量 spawn | 宿主只造 `SpawnCmd`（新音符）→ device 端 append + commit 更新 S_LIVE | ✅ `spawn.wgsl`、`driver.rs::materialise` |
| note-off 反查 | 宿主每块发布**本块** note-off runs（off_meta + off_runs）；voice 设备端二分自己的 release 帧；跨块旧 note-off 由"ordinal ≤ 块首计数"直接判定为已释放 | ✅ `voice.rs::GateTable`（end_block 每块重建 runs） |
| 控制器 | 每通道 8 字段行（bend/gain/variant/cut/id）；**变体按需重建**（控制器变化时，O(channels)），设备端每 gate-tile 指针切换 | ✅ `ChannelTable`、`driver.rs::pending_variants`、`render.wgsl` CHAN_VARIANT |
| 混合 | render 段内 workgroup 共享内存树归约 → `partials[sample][wg]`；reduce 两形态（nwg≤64 thin / 否则每样本一工作组），Kahan 可选；partials 固定 64 MiB（2048 wg × 8192 样本 × 4 B） | ✅ `render.wgsl::reduce_into_partials`、`reduce.wgsl`、`mod.rs::submit()` |
| 每块排序 | 按 (region, stage, phase) 的 **1-bit radix，`sort_key.bits` 遍全跑**（典型大音色库 ≈ 32；bits = region + 3 + phase）；gather 整池搬运 26 字段/voice；目的=采样局部性 | ✅ `SortKeyLayout::plan`、`sort.wgsl`、`compact.wgsl`；`sort_voices` 可关 |
| 抢占/准入 | steal **仅池过载触发**（8 遍直方图选择阈值，O(V)）；准入在宿主、仅针对**本块新音符**（分层 `select_nth` + 几何抽稀 + 多线程 materialise）；steal-percent=25% 封顶 | ✅ `plan_spawns`、`select.wgsl`、`rank_candidates` |
| TDR 规避 | 块 > submission budget（默认 4,194,304 声部，3× 默认池）→ 按 workgroup 范围切多次提交（~6 ms/边界，不改一个 bit）；预算自适应（主机等待 >0.4 s 减半 / 16 安静块翻倍） | ✅ `render_parts`、`submit()` parts 循环 |
| 确定性 | 无浮点原子；固定顺序树归约；稳定基数排序；相位 32.32 定点（2×u32 通道，WGSL 无 u64）；包络 level 量化（4114...=2^22 步） | ✅ `common.wgsl`、DESIGN |

### 3.3 每块成本模型（V=活跃声部，B=4096）

- 宿主：O(新事件) + O(channels/gates) + **64 B 回读**；稳态对 V 零成本（✅ `submit_block/finish_block`）
- 设备：steal 0（未过载）｜spawn O(新增)｜render Θ(V×B)（寄存器状态、packed 16-bit 采样、每帧零全局加载）｜reduce ≈ B×V/256 读｜sort+gather ≈ 32×V + 26×V 字搬运（<1% 渲染工时）
- 已知弱点：**宿主单核 MIDI 解析**（十亿级 note-on 要 minutes，自述瓶颈，Planned 跨核化）；纯离线无实时

---

## 4. LGS 现状架构（✅ 代码级）

### 4.1 每块管线（`render_block`，实时/离线共用）

```
apply_events → collect_pending_readback(映射上一块) → [静音快路径]
→ sync_voice_states(O(V)) → upload_voices(全量重建+上传) → update_mix_params
→ dispatch(render + mix 一次提交) → readback(音频+全量状态)
```

### 4.2 关键事实

| 事实 | 位置 |
|---|---|
| 每块全量重建+上传：`VoiceParams` 116 B + `VoiceState` 68 B + env ≤96 B + chans 4 B ≈ **284 B/声部/块**；回读 68 B/声部/块（`STATES_SYNC_EVERY=1`） | `pool.rs`、`readback.rs`、`dispatch.rs` |
| render 段：`grid=(⌈voices/128⌉, SEGS=4)`，段用 `advance_frame` **逐帧重放**快进（+150%；每声部每块 advance 1280 次 vs 需要 512） | `render.wgsl:325-336`、`gpu.rs:43` |
| 帧热路径：每帧一次 `env_stages` storage 读 + 一次 f32 除法；采样 f32、4 chunk 分派 | `render.wgsl:219,226-227,114-134` |
| mix：**并行度=块大小**（block/128 个 WG）；每线程串行扫全声部；累加器/状态机 ≈17.4 KiB 私有内存；事件重放 O(block×events) | `mix.wgsl:100-158`、`dispatch.rs:207-215` |
| `voice_out`：渲染整块写（8 B/帧/声部）、mix 整块读；容量 = 2 GiB − 64 KiB（512→~52.4 万、4096→~6.55 万声部） | `render.wgsl:396-398`、`mix.wgsl:112`、`types.rs MAX_VOICE_OUT_BYTES` |
| 块长默认 512（`config.rs:167`）；**导出硬编码 512**（`export/.../synth_config.rs:59`） | — |
| 实时治理：Governor 四级 + 运行时声部上限（**"每声部每块 ≈1 µs，collect+upload 主导"**，512 块 12288 声部饱和）+ 防爆闸 + 死机制 `flush_epoch` | `drain.rs:79-99`、`start.rs:141-147` |
| 已知放大器：CC72/73 **O(事件×V)** 全声道重建、不判值变 | `cc.rs:32-51` |

### 4.3 一级病根（与 Kestrel 对照）

① 宿主每块 O(V) 往返 ② mix 并行形状 + voice_out 两趟 ③ 渲染内核常数（重放/全局读/除法/带宽） ④ block 硬编码+容量耦合 ⑤ 实时注入通道（每事件 Mutex）。

---

## 5. 差距分解（每块记账）

| 项 | LGS 现状 | 18 卡目标 | Kestrel |
|---|---|---|---|
| 宿主 | O(V)×~284 B 上行 + 68 B 下行 | O(V)×~120 B（仍 O(V)） | **O(ΔV) + 64 B** |
| mix | O(B×V) 读×2 趟，并行度 = B | O(B×V/CHUNKS)+reduce，读 partials | O(B×V/256) 树|
| render | O(V×B)×2.5 推进 + 重常数 | O(V×B) + 轻常数 | O(V×B) + 轻常数 |
| 状态回读 | O(V)×68 B | 未覆盖（红线约束） | **64 B** |
| 容量 | 2 GiB/(8 B) ↔ block 耦合 | 解耦（002） | 池独立于 block |

**注意口径**：Kestrel 也只是 O(V)（渲染主项 + sort/compact），它赢在常数、访存、并行形状、宿主参与度四个维度——**没有 O² 可砍，只有 30~78× 的斜率可砍**。

---

## 6. 残余缺口（18 卡未覆盖，建议后续立项）

| 缺口 | 现状 | 建议 |
|---|---|---|
| 状态回读下载侧 | 每块 V×68 B map+copy+merge；`STATES_SYNC_EVERY=1` 为历史红线 | 多做"仅回读 ended 位图"的最小版；或与维护者重新裁决红线 |
| 宿主动态组装 | 010/011 只瘦身不消除（仍每块 O(V)） | 远期：对齐 Kestrel 增量模式 |
| 容量/内存 | streaming 路径有 100 MB 硬门（`streaming.rs:207` 调 `check_memory`，~20 万声部触顶）；主路径无门但 `Voice`(~300 B/声部) + 上传数组吃 RAM | 复核并统一口径；测算 `Voice` 真实尺寸 |
| CC72/73 放大器 | O(事件×V)、不判值变 | 最小修复（值未变跳过）+ 进阶（kestrel 式通道变体指针） |
| 实时阈值重标定 | `voice_limits_for` 1 µs/声部模型、12288/8192/16384 | 001/002/005 落地后重测重标 |

---

## 7. REF-LGS 18 卡映射与执行序

| 轨 | 卡（优先级/工作量→目标） |
|---|---|
| mix 链 | 003（M/L，+7%）→ **001（H/H，0.541→≥1.2×）** → 002（M/H，@4096 1.39→≥1.8） |
| 块长链 | 008（M/M，bug 修复，无增益）→ 009（M/L，默认 2048 ≥1.2×） |
| 内核链 | **005（H/M，≥0.75×，advance 1280→512）** → 006（M/L，≥0.57×） |
| 独立 | 007（M/M，带宽/显存 ÷2，≥0.60×）｜010（M/M，upload ≤3 ms，+10%）+011（L/M，env 96B→4B，+5%）｜004（L/M，高 NPS 条件 +15%）｜012（H/M，实时注入）｜013（H/M，基准护栏）｜014（L/M，采样单份） |
| 质量批 | 015（H/L，limiter 崩溃）｜016/017/018（L/L） |

依赖硬约束：003→001→002（同文件）；005→006（同函数）；**008→009**；013 建议最先；018 建议在管线重构前。

---

## 8. 红线清单（改动时严禁触碰）

- `STATES_SYNC_EVERY = 1`（`types.rs:85`，"历史明确禁止"——但禁令出处未考证）
- `render.wgsl` 的 `prog clamp`（`:92`）与 0 除守卫（`:226`）——历史 9.2e8 爆音教训
- `GrowableBuffer` 清零语义（`gpu/buffer.rs:45-62,157-158`）——历史 ~40000 次单样本爆音
- `voice_chans` 位打包（`channel | released<<8`）、filtered 声部的单段退化
- limiter 的 0.5 ms attack / 80 ms release / 0.98 阈值；`LOOKAHEAD=256` 与块长下界的约束（015）
- 逐位一致口径：除 007（采样量化，允许 <-90 dBFS 有界差异）外，全部改动要求逐位一致

---

## 9. 已知未知（接手前必读）

### 9.1 代码未实读（本研究覆盖 ≈ 热路径全骨架，其余未读）

- **LGS 侧 ~3,500 行**：`voice_alloc.rs`、`events.rs`（spawn/事件热路径）；`init.rs`、`limiter.rs`（015 现场）；`channel.rs`、`warm.rs`、`progress.rs`；`dsp/*`；`soundfont/*`；`midi/*`；`audio/playback/{admission,api,stats}`（防爆闸本体、017 漂移现场）；`compare.rs`；应用侧 `midi-io/api/lgs.rs` 全文、`runner/midi_manager/*`
- **Kestrel 侧 ~8,000 行**：`driver.rs` 余部、`limiter.rs`、`bank.rs`、`midi.rs`、`cpu.rs`、`gpu/{batch,budget,device,vram}`、TUI/falconeye、`comments/` 注释全集

### 9.2 未做的实验

- 全部基准数字**无独立复测**（来源：外部对比表 / 卡内自报 / Kestrel 自述）
- GPU pass 级时间分解（LGS 目前无 timestamp——即 REF-LGS-013 范围）
- `render_block` 调用树的系统性静态扫描（CC 放大器系手捡）
- 极限内存/容量实验（100 MB 门、`Voice` 尺寸未测算）
- 实时路径 profiling（设备重采样、锁争用实测）

### 9.3 未碰的维度（本分析只覆盖性能）

- 正确性：008 根因未调查；XSynth 对拍测试现状未盘点（`offline.rs` 自述两条入口逐字节一致仅人工 md5）
- 音质/DSP（包络/滤波/sinc/限幅精度）；Governor 实际行为验证

### 9.4 工程注记

- Kestrel 为 **MPL-2.0**（文件级 copyleft）且作者自述"100% AI 编写、无人逐行 review、验证套件不在仓库"——**只借方法论与形状，不搬代码**
- 基准环境：RTX 2060 Laptop（发热降频未控制）、Kestrel 1.3.0、LGS `profile_gpu` 口径
- 开放设计问题：① `RENDER_SEGMENTS=4` 在 005 落地后的去留（段并行的价值重评估）；② `STATES_SYNC_EVERY` 禁令的出处与解除条件；③ `check_memory` 只在 streaming 路径——是否统一

---

## 10. 靶点地图（文件:行号，✅=实读）

- **mix**：`mix.wgsl:56,100-103,111-123,127-128,134-158,165-168,180-181` ✅；`dispatch.rs:207-215` ✅；`bind.rs:12-40,82-87,115` ✅
- **render**：`render.wgsl:114-134,137-158,219,226-227,288-306,325-336,396-398` ✅；`gpu.rs:26-50,43` ✅；`pipelines.rs:20-33` ✅
- **上传/回读**：`pool.rs:4-250,100-170,126-132,145` ✅；`readback.rs:14-103,181-208` ✅；`dispatch.rs:74-78,139-183` ✅；`types.rs:85` ✅；`layout.rs:11-41,51-71,86-90` ✅
- **块长/配置**：`export/.../synth_config.rs:59` ✅；`config.rs:167,349-396` ✅
- **limiter/设置**：`limiter.rs:18` 📌；`ui-settings/.../audio/lgs.rs:56` 📌
- **实时**：`midi-io/api/lgs.rs:139-145` 📌；`start.rs:141-147,170-194` ✅；`drain.rs:79-99` ✅
- **CC 放大器**：`cc.rs:32-51` ✅
- **采样域**：`render.wgsl:64-72` ✅、`upload.rs:9-99` ✅、`resample.rs` 📌
- **资源**：`soundfont/load.rs:26-27` 📌；`midi-io/api/lgs.rs:277-304` 📌

---

## 11. 术语对照

| LGS | Kestrel | 说明 |
|---|---|---|
| `voice_out` | `partials` | LGS per-voice 整块落地；Kestrel per-workgroup per-sample 部分和 |
| `mix` pass | `reduce` pass | 归约与通道增益；Kestrel 的通道增益在 render 内按 gate-tile 应用 |
| `release_at`（烘焙进 params） | gate 表 + 设备端二分 | note-off → release 帧的两种实现 |
| `upload_voices` 全量 | `SpawnCmd` 增量 | 每块状态搬运范式差异 |
| `STATES_SYNC_EVERY` | — | LGS 全量状态回读节流（=1）；Kestrel 无对应物（64 B 摘要） |
| Governor / 防爆闸 | — | LGS 实时专属治理；Kestrel 无（离线无 deadline） |

---

## 附：本文档的边界

- 本分析**只覆盖性能/架构**；结论基于读码 + 外部数据，未做独立实测（见 §9）
- Kestrel 的一切细节以本地副本 `E:\kestrel-midi`（main）为准；行号会随版本漂移
- 引用 Kestrel 机制时请注明"仅方法论参照"（MPL-2.0）
