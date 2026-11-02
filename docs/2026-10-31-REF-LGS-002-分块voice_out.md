# REF-LGS-002 · 分块 voice_out（bit-exact 显存/容量优化）

> 日期：2026-10-31 ｜ 机器：RTX 5080 / 驱动 610.88 / 48 kHz
> 结论：**逐位一致**；512/2048 档速度不变，4096 −1.9%（阈值内），8192 +0.5%；
> `voice_out` 峰值：4096 **929 → 465 MiB（−50%）**，8192 **~1.86 GiB → ~486 MiB（−75%）**；
> 8192 的容量上限从 **32,767 → 131,072 声部（4×）**。

---

## 1. 为什么不是卡内的“渲染内归约”

002 卡内方案是“render pass 内按通道 workgroup 归约 + partials”，目标是消除
`voice_out` 的写+读。但跨声部归约会改变浮点加法顺序（树形/分组 ≠ 顺序左折叠），
无法满足仓库“同块长/同配置逐位一致”的红线；001 已经证明该红线可以靠“保序并行”实现。

同时，001 落地后 002 的**速度目标（4096 ≥1.8×）已被超额完成**：实测 4096 = **9.9×**。
因此本卡改为**只优化显存/容量**：把 block 切成帧块，复用同一块 `voice_out`。

## 2. 设计：帧块复用 voice_out

- `MIX_CHUNK_FRAMES = 2048`；`chunk = render_chunk_size(block) = clamp(block, 16, 2048)`。
  ≤2048 档 `chunk == block`，路径与原实现完全一致；4096/8192 才分块。
- CPU 每个 block 循环 `chunks = block / chunk` 次，每个 chunk 依次 dispatch：
  1. render：`chunk` 帧 × 全部声部，写 `voice_out[(v*chunk + f)*2]`；
  2. mix gather：001 的稳定通道桶，读同一 `voice_out`；
  3. reduce：按通道升序写 `out_storage[(mix_frame_base + f)*2]`。
- `voice_out` 只按 `chunk × voices × 8 B` 分配/增长；峰值不再随 block 线性放大。
- **状态连续性**：render 在每个 chunk 末尾把 `states[voice]` 写回；下一个 chunk 从该状态
  继续。`advance_frame` 的逐帧推进顺序不变，所以音频逐位一致。
- **ChunkParams（动态 uniform 偏移）**：`render_frame_base`（全局帧，用于
  `start_at`/`release_at`）、`mix_frame_base`（块内相对帧，用于 `MixEvent` 与
  `out_storage` 索引）、`chunk_size`。每个 chunk 一个 16 B 槽位，bind group 只建一次，
  `set_bind_group(0, bg, &[slot_offset])` 切换。
- **容量上限**：`MAX_VOICE_OUT_BYTES` 的裁剪判据从 `block × voices × 8` 改为
  `chunk × voices × 8`。8192 档上限从 32,767 提升到 **131,072 声部**。
- `mix_partial` 也按 `chunk` 分配（24 B/帧/通道）。

## 3. 逐位验证（全部与 pre-002 一致）

| 用例 | FNV-1a 64 |
|---|---|
| `sustained65536 @512` | `a3397a2279e0a11f` |
| `sustained65536 @2048` | `8579a7c425839446` |
| `sustained65536 @4096` | `495445713f3428ec` |
| `sustained65536 @8192` | `807cb1e2b19ee10a` |
| `multiport_16p_256ch @4096` | `aa0742fac826b05f` |
| `gate_2p_32ch @4096`（32ch） | `bcb2b82d151dfe9f` |
| `scatter100k @512`（no limiter） | `1ad03ca435f4783a` |

> 2048/4096/8192 都跨了 chunk 边界，且 256ch/门控也覆盖；状态屏障与 mix 事件重放均验证通过。

## 4. 性能 A/B（双二进制、交替、6 对中位、gate 2%）

| 用例 | pre-002 | post-002 | delta | 判定 |
|---|---:|---:|---:|---|
| `sustained65536 @512` | 3.481 s | 3.474 s | **+0.19%** | ok |
| `sustained65536 @2048` | 1.330 s | 1.305 s | **+1.86%** | ok |
| `sustained65536 @4096` | 0.986 s | 1.005 s | **−1.89%** | ok（阈值内） |
| `sustained65536 @8192` | 0.836 s | 0.832 s | **+0.53%** | ok |

调参记录：先试 `MIX_CHUNK_FRAMES=1024`，4096 −11%、8192 −14%（chunk 间状态串行 +
每 pass 固定开销）；改 2048 后回到阈值内，仍保留 4× 容量收益。

## 5. 显存 / 容量记账

| block | chunk | 峰值声部 | voice_out pre-002 | voice_out post-002 | 降幅 |
|---:|---:|---:|---:|---:|---:|
| 512 | 512 | 29,489 | 115 MiB | 115 MiB | 0 |
| 2048 | 2048 | 29,578 | 462 MiB | 462 MiB | 0 |
| 4096 | 2048 | 29,734 | **929 MiB** | **465 MiB** | **−50%** |
| 8192 | 2048 | ~29.8k | **~1.86 GiB** | **~486 MiB** | **−75%** |

容量：8192 档 `MAX / (chunk × 8)` = **131,072 声部**（原 32,767，4×）。

## 6. 与卡内验收对照

| 验收 | 状态 |
|---|---|
| 4096 ≥1.8× 倍实时 | ✅ 9.9×（001 已达成；002 后仍 ~9.7×） |
| 音频逐位一致 | ✅ §3 |
| `voice_out` 从 binding/init 彻底移除 | ➖ **未做**：逐位一致需要逐声部中间值；本卡把它从 `block` 缩到 `chunk`（4096 −50%、8192 −75%） |
| 显存峰值下降 = `block × 峰值声部 × 8 B` | ➖ 实际下降 `(block − chunk) × 峰值声部 × 8 B`（见 §5） |
| 多端口/单端口无回归 | ✅ 256ch @4096、gate @4096 逐位一致 |
| `cargo test` 全绿 / fmt / clippy | ✅ 95/95，clippy 零警告 |
| 实时 2k/8k underrun | 未在本机自动测（同一 GPU 路径；离线 A/B 为代理） |

## 7. 已知代价与后续

- **chunk 间状态串行**：render 的 chunk N+1 依赖 chunk N 的 `states` 写回，GPU 无法跨
  chunk 重叠；这是 4096 −1.9% 的主要来源。若未来要彻底消除，需要“渲染内有序归约”，
  即回到非逐位方案（需容差）。
- **`MIX_CHUNK_FRAMES` 是唯一旋钮**：1024 = 更多显存收益、−11~14%；2048 = 阈值内、4× 容量；
  4096 = 仅 8192 分块（2× 容量）、4096 完全不变。
- 容量上限已从 32,767 提升到 131,072（8192 档），后续 009（默认块长提升）可受益。

## 8. 改动文件

| 文件 | 改动 |
|---|---|
| `gpu.rs` | `MIX_CHUNK_FRAMES=2048`、`render_chunk_size`、chunk binding 常量 + 单测 |
| `gpu/layout.rs` | `ChunkParams`（16 B） |
| `gpu/pipelines.rs` | `bind_entry_dynamic_uniform` |
| `gpu/resources.rs` | render/mix layout 增动态 uniform；按 chunk 编译 render pipeline；`chunk_size` 字段 |
| `gpu/shaders/render.wgsl` | `ChunkParams`；帧基址改用 chunk；注释按 chunk 更新 |
| `gpu/shaders/mix.wgsl` | `ChunkParams`；`chunk_size`/`frame_base`；事件重放用块内相对帧 |
| `gpu/shaders/mix_reduce.wgsl` | `ChunkParams`；写 `out_storage` 的全局块内偏移 |
| `synth/engine.rs` / `init.rs` | `chunk_params_buf`（动态偏移槽）、`voice_out`/`mix_partial` 按 chunk 分配 |
| `synth/engine/bind.rs` | render/mix bind group 增动态 uniform 绑定 |
| `synth/engine/dispatch.rs` | 按 chunk 循环三 pass；容量判据改用 chunk |
| `scripts/lgs_bench_cases.json` | 新增 4096/8192 用例 |
