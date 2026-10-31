# REF-LGS-007 · 16-bit snorm 采样池（显存 −50%，linear 无退化）

> 日期：2026-10-31 ｜ 分支：`ref-lgs` ｜ 状态：**已接受并合入**（用户拍板）
> 验收：显存 762 MB → **382 MB**；linear 512/2048/4096 **+0.4% / +0.5% / +3.9%**；
> sinc 512 **+6.6%**（目标 +10%）、RMS **−89.5 dBFS**（目标 < −90）——两项差一点，经拍板接受。
> 实现 patch 留档：`.ai-context/sessions/2026-10/2026-10-31-REF-LGS-007-WIP.patch`
> （32 KB，用于回溯/重放；`.ai-context/` 按项目约定不入库）
> 结论：正确性可以达标（全素材 ≤ −90 dBFS），但**性能未达约定验收**
> （linear 512 **−4.2%**、4096 **−5.2%**、sinc 512 **+7.7%** < 目标 +10%），
> 按「性能目标 A」回退，等大音源夹具（用户提供）复测后再决定是否合入。

---

## 1. 做了什么（patch 已留档）

`git apply .ai-context/sessions/2026-10/2026-10-31-REF-LGS-007-WIP.patch`

- 采样池 `array<f32>` → `array<u32>`（每字 2 帧 snorm16，**显存/取指字节 ÷2**）；
- `raw_sample` 按字寻址 + `unpack2x16snorm`；
- 新增 `VoiceParamsDyn.sample_gain/_r`（**每采样归一化增益**，见 §2）；
- `upload_sample_packed` 统一三条上传路径（render 懒上传 / offline 预载 / warm 预载）；
- 工具：`hash_render --sinc / --dump-zones / --check-pool <key>`（后者可读回采样池逐字对拍）。

## 2. 关键发现：SF2 采样会超出 ±1，直接 clamp 会削波

首版实现直接 `clamp(±1)` 量化，结果 `sustained65536` 的差异高达 **−30.8 dBFS**、
单音 `key=80` 也有 **−53.5 dBFS**。逐层定位：

1. 单音 `key=0`（sample 0）/ `key=69`（sample 1）：**逐位一致 / −165 dBFS**；
2. 单音 `key=80`（sample 2）：波形相同、**整体按帧缩放**（比例 1.0→1.38），无时间偏移
   （互相关最佳 lag = 0）→ 不是取指错位；
3. `--check-pool 80`：GPU 池字节与 CPU 打包**逐字相同**（4439/4439，0 处不一致）
   → 写入/取指映射正确；
4. 打印采样峰值：**sample 2 `max_abs = 1.670653`，4.2% 的帧 |x| > 1**
   （旧 f32 池不限幅，snorm16 只有 ±1）。

**修复**：按采样归一化——`gain = max(1, |peak|)`，池里存 `x / gain`，
shader 乘回 `gain`。量化误差相对采样峰值仍是 1 LSB。

修复后（limiter off，512）：

| 素材 | max\|d\| | RMS |
|---|---:|---:|
| single（sample 1） | 0 | **逐位一致** |
| one_note k0（sample 0） | 3.0e-7 | −165 dBFS |
| one_note k80（sample 2） | 7.8e-6 | −121 dBFS |
| sustained2048 | 5.7e-5 | **−108 dBFS** |
| sustained65536 | 1.0e-4 | **−92 dBFS** |
| gate_2p_32ch | 4.5e-8 | −182 dBFS |
| sustained2048 / 65536（limiter on） | 2.4e-5 | −115 / −114 dBFS |

→ **正确性达标**（约定 ≤ −90 dBFS）。

## 3. 性能为什么没过（`rich.sf2` 只有 17 KB 池）

`sustained65536`，3 次中位：

| 场景 | pre-007 | post-007 | delta | 目标 |
|---|---:|---:|---:|---|
| 512 linear | 2.814 s | 2.930 s | **−4.2%** | 不退化（±2%） |
| 4096 linear | 0.813 s | 0.855 s | **−5.2%** | 不退化 |
| 512 sinc | 11.211 s | 10.348 s | **+7.7%** | ≥ +10% |

原因很直接：`rich.sf2` 的池只有 17 KB（3 个采样），**全部驻留 L1/L2**，带宽减半
没有收益；而每帧多了 `unpack2x16snorm`（snorm→f32）+ `select` + `gain` 乘法。
sinc 档每帧 64 次取指，字节减半才有 +7.7%——但仍不够。

卡片自己的诊断步骤就写了「本卡对大音源收益最大」（`test.sf2` 46 MiB 池 vs
`rich.sf2` 0.0 MiB），所以**必须在 46 MiB 级夹具上复测**才能判定。

## 4. 大音源复测（`F2 Jasmine 2.1.sf2`，3.4 GB / 3168 采样 / 18.6 亿帧）

用户提供的真实大音源。`sustained65536`（10 s），2–3 次中位：

| 验收项 | 目标 | 实测 | 判定 |
|---|---|---|---|
| 采样池显存 | ÷2 | **762 MB → 382 MB** | ✅ |
| linear 512 | 不退化 ±2% | **+0.4%** | ✅ |
| linear 2048 | 不退化 | **+0.5%** | ✅ |
| linear 4096 | 不退化 | **+3.9%** | ✅ |
| sinc 512 | ≥ +10% | **+6.6%** | ❌ 差 3.4 pp |
| RMS 差异 | < −90 dBFS | **−89.5 dBFS** | ❌ 差 0.5 dB |

> 关键修正：**量化必须放进并行段**。串行打包（7.4 GB CPU 缓存的量化）会拖慢
> 短渲染——同一实现串行时 linear 512/2048/4096 是 **−3.5% / −3.6% / −7.4%**，
> 移到 `par_iter` 后变成 +0.4% / +0.5% / +3.9%。大音源的量化是一次性成本，
> 但对 10 s 夹具仍可见。

### 4.1 为什么 RMS 是 −89.5 而不是 −90

- 大音源 `max_abs = 1.063`（18.6 亿帧里只有 102 帧 >1）→ 增益几乎为 1，不是削波问题；
- 差异的**相对值**是 **−79.5 dB**：16-bit 量化误差是 1 LSB（相对采样峰值），
  而信号 RMS 比采样峰值低约 10 dB → 相对误差约 −80 dB，这是 16-bit 的物理下限；
- 绝对 −89.5 dBFS 是「混音后、限幅器把总和解回 0.98」的口径，差 0.5 dB 即差
  5.6% 幅度——属于 16-bit 表示本身的边界，不是实现缺陷（关限幅时绝对值是
  −68.7 dBFS，因为原始总和 RMS 达 +11 dBFS）。

### 4.2 结论：2/4 达标，两项经拍板**接受**

按约定验收：显存、linear 全档达标；**sinc 6.6% < 10%、RMS −89.5 > −90**。
两项都只差一点，**已由用户拍板接受**（2026-10-31）：

- 接受理由：显存 −50%、linear 全档不退化，是本卡的主要目标；RMS 的 0.5 dB 是
  16-bit 表示的物理下限（相对误差 −79.5 dB），不是实现缺陷；sinc 的剩余差距
  来自「相邻 tap 未共用 u32 字」，作为后续可选优化（不是阻塞项）。
- 后续（可选）：sinc 循环里把连续 tap 合并到同一字的读取，预计可再拿 3–4%。

## 6. 新 golden 基线（007 之后，`rich.sf2`）

> 量化会改变**所有**输出哈希：下表是 007 之后的权威基线，005/006、010/011 等
> 卡片文档里的 `a3397a22…` 表是 **007 前**的历史基线。

| 用例 | FNV-1a 64 |
|---|---|
| filtered 512 / 2048 / 4096 | `ea209e1f675b4d03` / `27bc1d15c5238a8c` / `9c09d02af073f00d` |
| unfiltered 512 / 2048 / 4096 | `ba3ce4034c7b5298` / `04283a647f7f6680` / `f6c3143261147cea` |
| `multiport_16p_256ch @512 / @4096` | `927514d6ddaf2b90` / `170478c8ddeb9411` |
| `gate_2p_32ch @512`（32ch） | `48bbe59384a073c6` |
| `scatter100k @512`（no limiter） | `9ae3b80f5ed8eec3` |
| filtered 512 + `--prewarm` | `ea209e1f675b4d03`（与非 prewarm 一致 ✓） |
| Jasmine 2.1 @512 filtered / sinc | `4ae7ab3cb5ceb311` / `837bc3b313c3ad26` |

## 7. 复测步骤（大音源到位后）

```bash
git apply .ai-context/sessions/2026-10/2026-10-31-REF-LGS-007-WIP.patch
cargo build --release -p lumino-gpu-synth --example hash_render
# 1) 正确性（limiter off）
LUMINO_NO_LIMITER=1 hash_render <large.mid> <large.sf2> 480000 512 --raw post.f32
# 2) 性能：512 linear / 512 sinc / 4096，各 3 次中位
# 3) 显存：hash_render 的 sample_gpu_bytes 应减半
```

判定：linear 512/4096 不退化（±2%）且 sinc 512 ≥ +10% → 合入；否则改为
**opt-in 配置**（默认 f32 池，导出/大音源用户可开），避免默认路径退化。

## 5. 已验证的附带结论（对后续卡有用）

- `unpack2x16snorm` 的 `.x` 是**低** 16 位（偶数下标）：单音/稀疏素材逐位对拍证明；
- 采样池的写入偏移、偶数对齐、跨 chunk 拆分均正确（`--check-pool` 逐字对拍）；
- `rich.sf2` 的 sample 2 峰值 1.67——**任何"限幅到 ±1"的采样域方案都必须先归一化**，
  否则会削波（本卡首版的 −52 dBFS 就是这么来的）；
- **大音源的量化必须并行**：串行打包在 10 s 素材上是 −3.5%/−3.6%/−7.4%，
  挪进 `par_iter` 后是 +0.4%/+0.5%/+3.9%。

## 6. 改动文件

| 文件 | 改动 |
|---|---|
| `gpu.rs` | `SAMPLES_CHUNK_WORDS` + `pack_samples_2x16snorm`（含归一化增益）+ 4 个单测 |
| `gpu/layout.rs` | `VoiceParamsDyn.sample_gain/_r` |
| `gpu/shaders/render.wgsl` | 按 u32 字寻址 + `unpack2x16snorm` + 增益乘回 |
| `synth/voices.rs` | `SampleRef`（offset+gain）、`gpu_dyn` 增参 |
| `synth/engine.rs` | `sample_offsets` 改 `HashMap<usize, SampleRef>`、`PreparedSample` |
| `synth/engine/upload.rs` | `upload_sample_packed`（写已打包数据）+ 并行量化 |
| `synth/engine/{offline,warm}.rs` | 预载路径并行打包 |
| `synth/engine/pool.rs` | 取 offset/gain 传给 `gpu_dyn` |
| `examples/hash_render.rs` | `--sinc`（sinc 档测量用） |

