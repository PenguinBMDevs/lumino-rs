# PERF-001：Kestrel 对比 LGS —— 性能评估与 lumino 提升方案

- 日期：2026-10-17
- 对象：`kestrel-midi` 1.3.0（`D:\LuminoDevelop\Source\kestrel-midi`，第三方 GPU 离线合成器）对比 lumino 现役 LGS（`crates/audio/gpu-synth` + `crates/audio/midi-io/src/api/lgs*`）
- 方法：源码静态逆向（含 `comments/` 剥离注释复原）+ **同机同素材实测**
- 测试机：**NVIDIA GeForce RTX 2060 6 GB**（Turing / 1920 FP32 lane / 驱动 610.62）、Intel i7-9750H（6C12T）、24 GB RAM、Windows
- 结论一句话：**kestrel 的性能是"优秀"级别（本机上把 104 万持续声部跑到 1.57× 实时）；LGS 目前"可用但不算优秀"，其 GPU 计算内核的每声部每帧效率比 kestrel 低约 10–16 倍，端到端低 8.9–22.8 倍，且提升空间是结构性的、可明确指认的。**

---

## 1. 实测结果（同一台机器、同一素材、同一音源）

### 1.1 真实黑乐谱：`【黑乐谱】超ナイト・オブ・ナイツ.mid`（165,347 note-on / 27 轨），取前 100.01 s，`test.sf2`（30.8 MiB，287 presets / 12,618 regions），48 kHz

| 指标 | kestrel 1.3.0（Vulkan，block 4096，max-voices 1,048,576） | LGS（`profile_gpu`，block 512，无限复音） |
|---|---|---|
| 渲染循环耗时 | **6.87 s** | 27.24 s |
| **倍实时** | **14.55×** | **3.67×** |
| 含音源加载的整体墙钟 | 10.77 s（SF2 解析+上传 1.4 s） | 31.92 s（音源+mprewarm，34 ms MIDI 解析） |
| 峰值并发声部 | **8,882** | 6,410（每 25 块采样） |
| 声部 spawn / 抢占 / 丢弃 | 112,416 / **0** / **0** | —（无内建全局抢占，池不满） |
| 峰值单块耗时 | —（GPU 占 84%，host 侧 ~14%） | 13.1 ms / 10.67 ms 预算（load ≈ 1.23） |

> 关键：**kestrel 峰值声部更多（8,882 > 6,410）、释放尾巴更长（`test.sf2` 释放 30 s，kestrel `info` 推算"不抢占需 70,848 槽"），却快 3.97×。**

### 1.2 受控合成素材（kestrel `gen-assets`，两侧完全相同的 MIDI + SF2）

| 素材（峰值声部两侧基本相同） | kestrel | LGS | 差距 |
|---|---|---|---|
| `sustained2048.mid` + `rich.sf2`，峰值 ~930 | 11.01 s 音频 / **0.65 s = 17.02×** | 10.63 s 音频 / **1.060 s = 10.03×** | 1.70× |
| `sustained65536.mid` + `rich.sf2`，峰值 ~29.4k | 10.07 s 音频 / **0.82 s = 12.33×** | 10.0 s 音频 / **18.49 s = 0.541×** | **22.8×** |
| 同上，LGS 换 block 1024 | — | 11.18 s = 0.89× | 13.9× |
| 同上，LGS 换 block 2048 | — | 8.04 s = 1.24× | 9.9× |
| 同上，**LGS 换成与 kestrel 相同的 block 4096** | 12.33× | 7.18 s = **1.39×** | **8.9×** |
| `sustained1048576.mid`，峰值 470,612 声部 | 10.07 s / **6.42 s = 1.57×** | 未测（按斜率外推 ≈0.02×，且会在 52.4 万声部撞 `MAX_VOICE_OUT_BYTES`） | ~70× |

### 1.3 LGS 自身遥测（`LUMINO_PROFILE=1`）给出的开销结构

| 场景 | collect（等上一块 GPU） | upload（CPU 组装+上传） | dispatch（提交） | readback（回读+map） | 总/块 | 预算 |
|---|---|---|---|---|---|---|
| 6,410 声部 / block 512 | 2.79 ms | 1.90 ms | 0.41 ms | 1.05 ms | 6.78 ms | 10.67 ms |
| ~900 声部 / block 512（空载基线） | ~0.3–0.9 ms | 0.06 ms | 0.08 ms | 0.6 ms | ~1.10 ms | 10.67 ms |
| 29,150 声部 / block 512 | 24.0 ms | 7.8 ms | 4.3 ms | 0.88 ms | 39.6 ms | 10.67 ms |
| 25,764 声部 / block 4096 | 87.4 ms | 8.3 ms | 2.4 ms | 7.7 ms | 109 ms | 85.3 ms |

拟合成线性模型（LGS）：**每块 ≈ 0.15 ms 固定 + 1.03 µs × 声部数**（block 512）。
kestrel 同素材同峰值：每块 ≈ 6.95 ms / 29.4k 声部 ≈ **0.24 µs × 声部数**（且块长是 8 倍）→ 折合每声部每帧 **0.05–0.08 ns**（kestrel）对 **0.83 ns**（LGS，block 4096，纯 GPU）→ **纯 GPU 每声部每帧差约 10–16×**。

### 1.4 复现命令

```powershell
# kestrel（本机需 release+dev；见 §6 的 ATL 说明）
cargo build --release --features dev            # 首次会下载 static-dxc（约 25 MiB，慢）
.\target\release\kestrel.exe --force-cli gpu-info
.\target\release\kestrel.exe --force-cli info   "<midi>" -s "<sf2>" --block 512
.\target\release\kestrel.exe --force-cli render "<midi>" -s "<sf2>" -o out.wav --seconds 100

# LGS（离线测量入口，与实时同一条 render_block 代码路径）
$env:LUMINO_PROFILE=1
cargo run --release -p lumino-gpu-synth --example profile_gpu -- "<midi>" "<sf2>" 4800000 512
```

---

## 2. 架构对比（决定性能的六件事）

| 维度 | kestrel 1.3.0 | LGS（`crates/audio/gpu-synth`） | 差距来源 |
|---|---|---|---|
| **每块 pass 数** | 5 组（steal, spawn, render, reduce, compact），默认 `10 + 4·bits ≈ 138` 次 dispatch/块，1 次 submit | 2 个 pass（render + mix），1 次 submit | kestrel 多但都是 O(live) 且摊在 4096 帧上 |
| **渲染并行度** | 1 invocation = 1 声部 × 整个 4096 帧块；grid = Σ`ceil(live/256)`，上限 2048 WG | 1 invocation = (声部, 段)，`SEGS=4`、`SEG_LEN=128`；grid = `(⌈voices/128⌉, 4, 1)` | 两者接近；LGS 的 segment 是"用冗余换并行" |
| **混音/归约** | render 内用 workgroup storage 做两级归约写入 `partials[frame][wg]`（64 MiB），再 `reduce` pass 按固定顺序树归约；**从不落地逐声部音频** | `mix.wgsl` 是 **gather**：1 线程 = 1 输出帧，循环**全部**声部；grid 只有 `block/128` 个 WG（block 512 → **4 个 WG / 512 线程**，占整卡 ~1%）；每线程私有数组 ~17.4 KiB 且有 4 组累加器 | **最大单项**：并行度 + 私有内存 + `voice_out` 走两遍 |
| **声部内存** | SoA 单缓冲，26×u32 = **104 B/声部**，ping-pong 两份；`voice_out` 不存在 | `VoiceParams` 116 B + `VoiceState` 68 B + env stage 预留 96 B + `voice_chans` 4 B + `voice_out` block×8 B（512 时 **4 KB/声部**） | LGS 每声部 ~4.4 KB（512 块）vs 104 B |
| **样本域** | 16-bit snorm，1 个 u32 装 2 帧 → **2 B/样本**；≤4 个 buffer | f32 → **4 B/样本**；4 个 chunk（各 ≤1 GiB） | 2× 采样带宽/显存 |
| **状态推进** | 整块状态驻寄存器，一帧一条 add64 + 包络分支；无每帧全局读 | 每帧一次 `env_stages[...]` **12 B storage 读** + 每帧一次 f32 除法；每个 segment 用 `advance_frame` **重放**快进：每声部每块 512 实渲 + 768 重放（+150%） | LGS 每帧多全局读/除法，且 1.5× 冗余 |
| **滤波声部** | 与普通声部同一路径（寄存器内 biquad，ramp 交叉淡入） | biquad 状态信号相关 → **不能快进** → 该声部退化为单段（并行度 ×1/4），且 seg>0 仍先读 68 B states 再 `return` | 有滤波的音色（默认 `use_effects=true`）吃掉段并行 |
| **Note-on / 抢占** | GPU spawn shader + 索引分配器（无原子）；CPU 只建 92 B 命令 + 12 B 候选；steal 用 8 轮 256 桶整数原子直方图选第 k 个最安静声部；受害者均匀撒在块内 + 96 帧淡出 | 全 CPU：模板缓存 + per-key trim（cap+25% 迟滞、5 ms 淡出、每块 ≤128 组）+ 全局上限（默认 16384）+ Governor 运行时上限；无 GPU 侧分配 | 语义都自洽；kestrel 的 CPU 成本更低（12 B/note-on vs 284 B/声部/块） |
| **CPU 侧每块** | 只建命令 + 入场候选；materialise 4 线程（73.5 s 渲染里 4.5 s→1.2 s）；`prepare_block` 与设备重叠（GPU 77% busy） | 每块重建 4 个 Vec + 12 KB `MixParams`；每声部 284 B 上传 + 68 B 回读（`STATES_SYNC_EVERY=1`）；实测 29k 声部时 upload 7.8–9.9 ms | LGS CPU 侧占其块时间 ~18% |
| **确定性** | 设计即逐位可复现（无浮点原子、固定顺序归约、索引分配、稳定基数排序） | 与自身同步基线逐位一致，但**与块长相关**（block 8192 输出不同，见 `pipelined-readback-optimization.md`） | 影响可用的块长上限 |

---

## 3. 差异归因（把 8.9–22.8× 拆开）

1. **`mix.wgsl` 的 gather 结构（最大项，约占 block 512 时 GPU 时间的一半）**
   - 并行度 = `block/128` 个 workgroup：block 512 → **4 个 WG**（2060 有 30 个 SM，占用率约 1%）；
   - 每线程私有 `acc_l/acc_r/acc_rel_l/acc_rel_r`（4×256×4 B）+ `cur: array<MixStart,256>`（12 KiB）+ `frames`（1 KiB）≈ **17.4 KiB**，必然溢到 local memory；
   - 其中 `acc_rel_*` 与 `acc_l/acc_r` 在末尾被**相加后乘同一个 amp**（`mix.wgsl:180-181`），**语义上可直接合并**，等于白付 2 个数组 + 每声部 2 次加；
   - 实测证据：block 512 → 4096，LGS 从 0.541× 提到 1.39×（2.6×），而 `collect` 从 24.0 ms/512 帧降到 87.4 ms/4096 帧（每音频秒 46.9 → 21.3 ms）。这正是"并行度/固定开销随块长改善"的指纹。
2. **声部音频落地 `voice_out`**：每声部每帧 8 B 写 + 8 B 读（渲染写、mix 读），并且 512 块下每声部占 4 KB 显存。kestrel 用 workgroup 归约 + partials 完全避免。
3. **segment 状态重放**：`render.wgsl:325-336` 每个 segment 从块首重放 `advance_frame` 到自己的起点，block 512/SEGS 4 → 每声部每块多 768 次状态推进（有效 512 帧的 1.5 倍），其中每次还带一次 `env_stages` 全局读与一次除法。
4. **样本以 f32 存储**：2× 采样带宽（kestrel 用一个 u32 装两帧 16-bit snorm）。
5. **每帧全局读 + 除法**：`f32(s.env_t) / f32(denom)`（`render.wgsl:227`）与 `env_stages[p.env_base+stage_idx]`（`:219`）都在最内层循环。
6. **CPU 侧 284 B/声部/块上传 + 68 B/声部/块回读**：29k 声部时 upload 7.8–9.9 ms、readback ~0.9–7.7 ms，占块时间 ~18%；kestrel 只需 12 B/note-on 的候选与 92 B/命令。
7. **块长**：LGS 默认 512 帧（8 ms @64k / 10.67 ms @48k），固定开销 ~1.1 ms/块 → **即使 GPU 免费，倍实时上限也只有 ~9.7×**；kestrel 默认 4096 帧。

**公允性说明（对 LGS 有利/不利的部分）**
- 有利于 kestrel：kestrel 样本池是 16-bit 量化（LGS 是 f32）；kestrel 用固定点 32.32 相位。
- 有利于 LGS：kestrel 在本测试里持有更长的释放尾巴（`test.sf2` 30 s）→ 实际声部帧数更多；LGS 的 `mix.wgsl` 额外承担了通道 CC7/CC11/CC10 的逐帧 lerp 与 pan 三角函数，而 kestrel 把通道增益做成 gate-tile 粒度（32 帧）的斜坡。
- 两者插值口径一致（都是 linear）；LGS 的 sinc 模式（64 tap/声道）默认关闭。
- 单次运行数据，未做多轮取中位数；按经验这类测量 run-to-run 波动约 ±5%。

---

## 4. 双方性能评级

### kestrel 1.3.0 —— **优秀（Excellent）**
- 本机（RTX 2060，比其标称的 RTX 5060 Laptop 弱约 2.5×）即可把 **104 万持续声部跑到 1.57× 实时**、47 万并发声部；真实黑乐谱 100 s 用 6.87 s 渲完（14.55×）。
- 其性能不是靠"少干活"换来的：`--profile` 显示 GPU 占 84%（GPU-bound，非 host-bound）；region/stage/phase 三段排序把采样取指压到 L1 32 B sector 级别（作者实测：样本池 4 MiB → 145 ms、512 MiB → 147 ms，即与 DRAM 带宽无关）；`DESIGN.md` 里每个取舍都有对照实验（如"瓶颈在 L1 供 sector 速率"、"多一个声部字段 = 45M 音符渲染的 5%"）。
- 工程纪律同样优秀：dev 模式的 `gen-assets` / `--profile` / `null` 对拍、逐位确定性断言、按机器自适应的提交预算、TDR 拆块。**这是 lumino 应当照抄的度量与验收范式。**
- 需要指出的短板（与性能无关）：100% AI 编写、无人工逐行审查；离线专用（无实时）；SFZ/异形音源与畸形 MIDI 覆盖不足；本机因缺 ATL 无法直接链接（见 §6）。

### LGS —— **可用，但引擎效率不算优秀（Fair）**
- 好的一面：本机实测真实黑乐谱 100 s / 3.67× 实时、峰值 6,410 声部；`sustained2048` 下 10.03×；且这是"带逐声部 biquad、f32 样本、CPU 全量声部管理"的条件下取得的。实时侧治理层（Governor 四级、有界 drain、令牌桶准入、fade-steal、`Underrun` 0 验收）设计成熟，是它真正的强项。
- 不足的一面：**GPU 计算内核的每声部每帧效率只有 kestrel 的 ~1/10–1/16**（0.83 ns vs 0.05–0.08 ns），端到端差 8.9×（同块长）到 22.8×（LGS 默认块长）；`mix.wgsl` 的并行度只有整卡的 ~1%；每块固定开销上限（~9.7× @ block 512）说明它**不是"接近硬件极限"，而是"结构上离极限很远"**。
- 量化余量：把 §3 的 1/2/3/4/6 项按 LGS 自身遥测估算，**在不改音频语义的前提下，LGS 有 4–8× 的语音吞吐余量**（对应本机从 6,410 声部/1.0× 提升到 ~3–5 万声部/1.0× 的量级）。

---

## 5. lumino / LGS 提升空间（按收益/风险排序）

### P0 —— 重做 `mix.wgsl` 的归约结构（预计 2–5×，风险中低）
文件：`crates/audio/gpu-synth/src/gpu/shaders/mix.wgsl`、`src/synth/engine/dispatch.rs`、`src/gpu/pipelines.rs`

1. **最小改动版（推荐先做）**：把"1 线程 = 1 帧、循环全部声部"改成"1 线程 = 1 帧、只算 1/CHUNKS 的声部"，grid 改为 `(⌈block/128⌉, CHUNKS, 1)`，写 `mix_partial[chunk][frame][ch]`，再来一个极小的第二 pass 求和。CHUNKS = 8～16 → mix 的并行 workgroup 数从 4 提到 32～64，每线程循环次数降到 1/8～1/16。
   - 若要保持"逐位可复现"：部分和按固定顺序相加即可（浮点加法顺序固定 = 结果固定）。
2. **更彻底版**：把逐声部音频累加搬进 render pass（照 kestrel 的做法用 `var<workgroup>` 做两级归约），直接删掉 `voice_out`（省每声部每帧 16 B 流量 + 4 KB/声部显存）。
3. **顺手可做的两件小事**：
   - `acc_rel_l/acc_rel_r` 与 `acc_l/acc_r` 在 `mix.wgsl:180-181` 被同一个 amp 相加 → **合并成 2 个数组**（少 2×256×4 B 私有内存 + 每声部 2 次加）；
   - `MAX_CHANNELS=256` 的 `cur: array<MixStart,256>` 只在 `channel_count`（默认 16）内需要 → 把私有数组按 `channel_count` 上界（或把逐通道 lerp 状态搬到 mix_params 之外的独立小缓冲）以避免 12 KiB 无条件私有内存。

### P1 —— 消除 segment 状态重放（预计 1.5–2×，风险中）
文件：`src/gpu/shaders/render.wgsl:325-336`、`src/gpu/mod.rs:37`（`RENDER_SEGMENTS`）
- 位置推进是仿射的（`speed` 每块恒定），包络是分段闭式的（`env_count` 通常 4–7 段）→ **段起点状态可按段数 O(#stages) 直接算出**，不必逐帧重放 768 次。
- 退一步的折中：新增一个极小的"段检查点"pass，让每个声部只算 3 个段边界状态（4 个段共享），把每声部的重放从 768 次降到 ~3 次。
- 同时把每帧的 `env_stages[...]` 全局读与 `f32/env_t` 除法改成寄存器内递推（stage 起点/速率在段切换时算一次）。

### P2 —— CPU 侧每块成本（预计 1.3–1.8×，风险中低）
文件：`src/synth/engine/pool.rs`（`upload_voices`）、`src/synth/engine/bind.rs`、`src/synth/engine/dispatch.rs`
- **静态/动态分离**：`VoiceParams` 里 `sample_offset/sample_len/speed/amp/pan/b0..a2` 等在整个声部生命周期内不变 → 声部 spawn 时上传一次并常驻 GPU，每块只更新 `int_time/frac/env_*/biquad state`（68 B）。可把每块上传从 284 B/声部降到 ~68–100 B/声部。
- **env_stages 去重**：目前每声部预留 8 段 × 12 B = 96 B 的**副本**；改为"模板表 + 索引"（同音色同包络的两万个声部共用一份）→ 直接省 96 B/声部/块的上传与显存。
- **免重建**：`bind.rs` 每块重建 4 个 Vec + 12 KB `MixParams`（屎山报告 M4）→ 用持久化 staging / 双缓冲复用。
- **回读**：`STATES_SYNC_EVERY=1` 使每块回读 68 B/声部（29k 声部 = 2 MB/块，实测 readback 7.7 ms @block 4096）。若把"下一块的 steal/spawn 决策"所需的状态改成 GPU 侧自算（或让 CPU 只回读 4–8 B/声部的摘要），可再省 5–10%。

### P3 —— 采样域改 16-bit snorm（预计 1.1–1.3×，显存减半，风险中）
文件：`src/gpu/shaders/render.wgsl:114-134`、`src/synth/engine/init.rs`、`src/audio/resample.rs`
- 一 u32 装 2 帧（kestrel 已证明可行且是其设计支柱），采样带宽/显存各减半；chunk 寻址的 `>>28 / &mask` 逻辑不变。代价是量化噪声（对 SF2 的 16-bit 源基本无损失，需用对拍确认）。
- 顺带把 4 个 ≤1 GiB 的 chunk 上限从"每 chunk 1 GiB f32"变成等效 2 GiB 采样。

### P4 —— 块长与固定开销（预计 1.5–2.6×，需先修正确性，风险中）
- 本机实测 LGS：block 512 → 1.39×（4096）。当前默认 512 是为实时延迟（10.7 ms @48k）服务的；但**实时默认采样率已是 64 kHz、默认块 512 = 8 ms**，而固定开销 ~1.1 ms/块（13.7%）仍偏高。
- 建议：实时保持 512，但把**离线导出的默认块提到 2048–4096**；同时修掉 `pipelined-readback-optimization.md` 记录的"block ≥8192 输出与基线不一致"的块边界 bug（不修就不能用大块）。
- 固定开销本身也要压：合并 `queue.write_buffer` 调用、避免每块 map/unmap 两次、把 state 回读降频（见 P2）。

### P5 —— 把引擎已有能力接进应用（不提升吞吐，直接消掉"现场卡顿/丢音"）
- `prewarm_midi_file` / `warm_gpu` / `prefetch_samples`：**引擎已实现但应用从未调用**（`.ai-context/sessions/2026-09/2026-09-18-lgs-dataflow-research.md:13-16, 129-132`）。REND-016 遗留的"样本懒重采样在渲染线程同步执行，首次卡 130–150 ms"正由此而来。
- `set_events` / `play_events` 批量注入：现在应用走"逐事件 `Mutex` + unbounded mpsc 单事件通道"（屎山 L8，`crates/audio/midi-io/src/api/lgs.rs:139-145`），黑 MIDI 下积压可达 17 万事件/秒。改成"按块批量注入"（一次取用发送端）即可。
- 这两个是"体感"层面最划算的改动。

### P6 —— 建立性能护栏（0 风险，长期收益）
- LGS 目前**没有任何自动化性能基准**：唯一证据是 `.ai-context` 与 `docs/` 里的历史记录，而且已经出现**文档与代码不一致**（admission 的 NPS 阈值：文档 32k/8k 与 128k/64k 两说，代码是 64k/32k；声部上限文档 65536/32768、代码 16384/8192）。
- 建议照 kestrel 的范式补：`cargo bench`/`--example` 固化 2 个体裁（持续声部 MIDI + 真实黑乐谱 30 s 片段）+ 固定 SF2，输出**声部帧/秒、每块分相预算、倍实时、underrun** 四项，进 CI 并保留基线。
- 顺带把 `GpuBufferStats` 补上"采样显存/CPU 采样缓存字节"（`docs/2026-10-17-数据源冗余审计.md:255-257` 已点名），否则显存回归无法观测。

### 附：扫描中发现的正确性/卫生问题（与性能无关但建议一并处理）
1. `limiter.rs` 的 `LOOKAHEAD = 256` 与设置页允许的 `block_size = 64/128` 冲突（`block_size*2 < 512` 时切片下溢）——屎山 H2 当时以"LGS 未接线"降级，**现已接线，等级应上调**。
2. `start.rs` 的过载重同步清队列机制在 `7bb878b9` 后 `flush_epoch` 无任何递增点 → 死代码/永不触发（REND-017 记录待裁决）。
3. `sf2` 采样数据在进程内可能有两份（LGS 一份 + 导出侧一份，再叠加 GPU chunk）——`soundfont/load.rs:26-27` 无缓存。
4. 文档-代码漂移（见 P6）；`readback.rs:118-121` 的"64 帧/1 ms"与 `limiter.rs:18` 的"256 帧/4 ms"不一致。
5. `gpu-synth` crate 刻意不继承工作区 lint（`Cargo.toml` 无 `[lints] workspace = true`）、沿用 `unwrap/expect` 上游风格——与 `AGENTS.md` 的强约束冲突，建议至少加一档 `clippy` 白名单并登记。
6. `render.wgsl:288 vs :298`：filtered 声部的 seg>0 会先读 68 B `states[voice]` 再 `return`，白费带宽。
7. Mono 输出被 `validate()` 拒绝、`set_percussion_mode` 为 no-op、bank/preset 硬编码 `load_soundfont(path, 0, 0)`（REND-013 #107）——能力缺口而非性能。
8. `crates/audio/midi-io/src/api/lgs.rs` 每个事件加锁（同 L8）；`lgs/stats.rs` 的快照只在 `new()` 与端口重建分支刷新，UI 侧需注意。

---

## 6. 口径与不确定度

1. **kestrel 的构建不是原始产物**：本机未安装 ATL，`kestrel` 链接时需要 `atls.lib`（`windows` crate 的 WIC/Imaging 模块引入，实际由静态链接的 DXC 使用）。测量所用二进制是用**同名符号桩**（`ATL::_AtlBaseModule`）链接成功的，只影响 **DX12 后端**；本次全部走 **Vulkan**（`gpu-info` 已确认适配器为 `Vulkan NVIDIA GeForce RTX 2060`）。若要用 DX12 或发布级复现，请安装 "C++ ATL for v143" 后重新 `cargo build --release`。
2. 渲染口径：两者同为 48 kHz、linear 插值、同一音源文件、同一素材、同一 GPU；kestrel 走 `--force-cli render`，LGS 走 `profile_gpu`（与实时同一条 `render_block` 路径，但不落盘）。kestrel 默认 `--limiter brickwall` 与 16-bit 样本池，LGS 为 f32 样本 + 逐声部 biquad + 无 headroom 管理的块级限幅——**两者的 DSP 口径并不完全等价**，本文的"倍实时"是端到端可比指标，而非等价工作量下的归一化指标。
3. 单次运行，未做多轮中位数；LGS 的 `voices` 是每 25 块采样（样本数 38–373 个/次），峰值可能低估。kestrel 的峰值是精确计数。
4. 每声部每帧的 ns 值由"峰值声部 × 块长"折算，属**上界估计**（平均声部数低于峰值），故 §1.3 的比例应视为量级而非两位有效数字。

---

## 7. 附：本次实测的原始数据位置

- `D:\temp\kestrel_night_100s.log`（kestrel 真实黑乐谱 100 s）
- `D:\temp\lgs_night_full.txt`（LGS 同素材 100 s）
- `D:\temp\lgs_frost_60s.txt`（LGS 密集素材前 60 s）
- `D:\temp\lgs_sustained2048.txt`、`D:\temp\lgs_65k.txt`、`D:\temp\lgs_65k_b1024/2048/4096*.txt`（受控扫描）
- `D:\temp\assets{,2,3}`（kestrel `gen-assets` 产出的可复现素材与合成音源）
- 静态扫描依据：kestrel `DESIGN.md`、`shaders/*.wgsl`、`src/gpu/*`、`comments/*`；lumino `crates/audio/gpu-synth/**`、`.ai-context/sessions/2026-08/**`、`docs/2026-09-19-LGS离线导出审查与修复记录.md`、`docs/2026-10-09-REND-016-*.md`、`docs/2026-10-11-REND-017-*.md`、`docs/2026-10-06-全仓屎山扫描-总报告.md`
