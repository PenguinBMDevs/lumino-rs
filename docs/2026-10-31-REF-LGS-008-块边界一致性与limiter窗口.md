# REF-LGS-008 · 块边界一致性与 limiter 绝对帧窗口（A2）

> 日期：2026-10-31 ｜ 分支：`ref-lgs` ｜ 机器：RTX 5080 / 驱动 610.88 / 48 kHz
> 拍板：**分层口径** + **limiter A2（绝对帧对齐固定窗口）** + **008/009 解耦**。
> 结论：渲染层跨块长差异 ≤ **−46 dBFS**（多数素材逐位相同）；limiter 的块长依赖被
> **完全消除**（`sustained2048` 从 96% 样本不同 / max 0.984 → −56.5 dBFS / max 0.118）。

---

## 1. 为什么"所有块长逐字节一致"不是可执行口径

卡内验收写的是"512/1024/2048/4096/8192 逐字节一致"。实测（`LUMINO_NO_LIMITER=1`，
按公共前缀逐样本比）存在**三层互不相同**的块长依赖：

| 层 | 机制 | 实测（512 vs 2048） | 能否消除 |
|---|---|---|---|
| **A. 声部裁剪的块粒度** | 死声部只能在块边界被 `retain` 剪掉，尾部滤波器余响的截断点随块长移动 | `single`：首差帧 2560，1228 样本，max **4.2e-18** | ❌ 除非按 512 子块渲染（=放弃大块收益） |
| **B. 块率治理** | `spawn_budget` 每块清零、每键 trim 的迟滞与淡出预算按块计 | `sustained2048`（关治理）：max 1.897；开启 global cap：**−17.6 dBFS / 94% 样本** | ❌ 属于块率语义，改了就是另一套行为 |
| **C. limiter 块末回退** | 256 帧 lookahead 窗口在块末截断，回退"整块峰值" → 增益随块长变 | 开限幅：首差帧 **304**，96% 样本，max **0.984** | ✅ **本卡已消除（A2）** |

A 层最轻的一档就已经是 4e-18：**逐字节一致在块渲染架构下不可达**。因此口径改为分层：

- **同块长**：逐位确定（含 limiter/治理），这是硬验收；
- **跨块长（渲染层参考模式）**：差异 RMS ≤ **−45 dBFS**，且只允许出现在音符结束的尾段；
- **跨块长（默认治理路径）**：不要求逐字节，只要求差异可解释、无爆音（治理是块率语义）；
- **limiter**：不得引入块长差异（A2 之后由绝对帧窗口保证）。

> `hash_render` 默认就是参考模式（`max_voices_per_key = 0`），额外用
> `LUMINO_NO_LIMITER=1` 关掉限幅器即可测渲染层。

## 2. A2：limiter 按绝对帧对齐的 512 帧窗口处理

旧实现把整个 render block 当作一个 limiter 单元，块末 256 帧的 lookahead 窗口
被截断时回退"整块峰值"——**窗口边界跟着渲染块走，增益轨迹就跟着块长走**。

新实现（`limiter.rs`）：

```rust
pub(crate) const LIMITER_WINDOW: usize = 512;

pub(crate) fn limit_stream(out, tail, gain, sample_rate, window) {
    // window = min(block_frames, LIMITER_WINDOW)
    for chunk in out.chunks_mut(window * 2) {
        limit_block(chunk, tail, gain, sample_rate);   // 增益/延迟线跨窗携带
    }
}
```

- render block ≥ 512（1024/2048/4096/8192，即 009 的目标区间）→ 都被切成**同一串**
  绝对帧对齐的 512 帧窗口，处理序列与 block=512 **完全相同**，因此 limiter 输出
  与 512 档逐位一致；
- render block < 512（实时小缓冲）→ 窗口 = 块长，走单次 `limit_block`，
  **行为与历史完全一致、零额外延迟**；
- 不引入跨 `render_block` 的音频缓冲，因此不需要环形缓冲，也不增加实时延迟
  （limiter 原有的 256 帧延迟线保持不变）。

### 2.1 不变量（`cargo test`，无需 GPU）

| 单测 | 断言 |
|---|---|
| `limit_stream_equals_per_window_calls` | 一段 2048 帧一次性处理 == 逐 512 帧调用 `limit_block`（输出/gain/tail 逐位） |
| `limit_stream_ignores_outer_block_split` | 一次 2048 帧 == 两次 1024 帧（跨调用携带 tail/gain 逐位） |
| `block_512_is_unchanged` | window=512 与改动前单次 `limit_block` 逐位相同 |
| `small_block_falls_back_to_single_call` | 16/64/128/256 帧退化为单次调用（实时零变化） |

## 3. 实测（`scripts/lgs_block_consistency.py`，安全带 = 2×最大块长）

### 3.1 渲染层（limiter off，`sustained2048`）

| block | 首个差异帧 | 差异样本 | max\|d\| | RMS | 判定 |
|---:|---:|---:|---:|---:|---|
| 512 | — | 0 | 0 | — | reference |
| 1024 | 12800 | 14022 | 0.336 | **−49.1 dBFS** | ok |
| 2048 | 12800 | 21827 | 0.336 | **−47.7 dBFS** | ok |
| 4096 | 12800 | 26559 | 0.336 | **−46.5 dBFS** | ok |

`gate_2p_32ch`：512/1024/2048/4096 **逐位相同**（0 差异）。
`single`：max 4.2e-18（−387.6 dBFS）。

差异全部落在音符结束的尾段（1024 个差异段，中位长度 1 样本、p90 12 样本），
来源就是 A 层（裁剪块粒度）。

### 3.2 默认路径（limiter on）

| 素材 | A2 前 | A2 后 |
|---|---|---|
| `sustained2048` 512 vs 2048 | 96% 样本不同，max **0.984** | **−56.5 dBFS**，max 0.118 |
| `sustained2048` 512 vs 4096 | 98.5% 样本不同（基线文档） | **−55.3 dBFS**，max 0.121 |
| `gate_2p_32ch` 512 vs 2048 | max 0.048 | **逐位相同** |

### 3.3 治理开启（global cap 2000，`sustained65536`）

512 vs 2048：**−17.6 dBFS / 94% 样本**——预期内，属于 B 层（块率治理），
不进一致性护栏，只要求"有界、可解释"。

## 4. 护栏

`scripts/lgs_block_consistency.py`：渲染各块长 → 落 raw f32 → 比较公共前缀
（去掉末尾 2×最大块长，规避"按整块停止渲染"造成的长度差），输出首差帧、
差异样本数、max|d|、RMS(dBFS)，并检查**同块长确定性**（参考块长跑两遍逐位相同）。

```bash
# 渲染层（默认关 limiter）
python scripts/lgs_block_consistency.py \
  --midi test-file/bench/sustained2048.mid --sf2 test-file/bench/rich.sf2 \
  --frames 480000 --blocks 512,1024,2048,4096 --json test-file/bench/block_consistency.json

# 默认路径（含 limiter）
python scripts/lgs_block_consistency.py ... --limiter --blocks 512,2048,4096
```

阈值默认 `--rms-dbfs -45`，可用 `--max-abs` 追加单样本上限。
`hash_render` 新增 `--max-voices N`（默认 0 = 无限），用于冒烟覆盖淡出路径
（`--max-voices 2000` @512 两次均为 `bc72c49bf646a0fb`）。

## 5. 验收对照

| 卡内验收 | 状态 |
|---|---|
| block 512/1024/2048/4096/8192 逐字节一致 | 🔁 **改口径**：渲染层 ≤ −46 dBFS；limiter 已消除其贡献（§3） |
| 与改动前 block 512 逐位一致 | ✅ filtered `a3397a22…`、unfiltered `9504b6e8…` 不变 |
| 块长一致性自动化用例（≥3 档） | ✅ `lgs_block_consistency.py` + 4 个 limiter 单测 |
| 实时路径无回归 | ✅ block ≤512 走单次 `limit_block`（单测覆盖 16/64/128/256/512） |
| fmt / clippy | ✅ clippy 零警告；105 单测 |

## 6. 已知边界（写进口径，不再当 bug）

1. **尾段截断**（A 层）不修：要修只能按 512 子块渲染，那等于放弃 009 的收益。
   差异只出现在音符结束后的滤波器余响，能量 ≤ −46 dBFS。
2. **块率治理**（B 层）不修：`max_voices` / `max_voices_per_key` 是块率语义，
   跨块长不强求一致（§3.3 给出量化）。
3. **block < 512** 的 limiter 仍是"一块一窗"：无法在不增加实时延迟的前提下
   跨块拼窗；512 档两者重合，所以边界处无跳变。
4. 逐位一致性检查必须用**公共前缀 + 安全带**：离线渲染按整块停止，
   文件长度本身与块长有关（不属于内容差异）。

## 7. 改动文件

| 文件 | 改动 |
|---|---|
| `synth/engine/limiter.rs` | 新增 `LIMITER_WINDOW` / `limit_stream` + 4 个单测 |
| `synth/engine/readback.rs` | `apply_limiter` 改用 `limit_stream(window = min(block, 512))` |
| `examples/hash_render.rs` | 新增 `--max-voices`（含输出/JSON 字段） |
| `scripts/lgs_block_consistency.py`（新） | 跨块长护栏（公共前缀 + RMS 阈值 + 确定性） |
