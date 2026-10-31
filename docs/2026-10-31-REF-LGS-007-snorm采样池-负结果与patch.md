# REF-LGS-007 · 16-bit snorm 采样池：已实现、按验收回退（负结果）

> 日期：2026-10-31 ｜ 分支：`ref-lgs` ｜ 状态：**未提交**（实现留档为**本地** patch）
> 实现 patch：`.ai-context/sessions/2026-10/2026-10-31-REF-LGS-007-WIP.patch`
> （`.ai-context/` 按项目约定不入库；设计细节与全部实测数据见本文，可据此重建）
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

## 4. 复测步骤（大音源到位后）

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
  否则会削波（本卡首版的 −52 dBFS 就是这么来的）。
