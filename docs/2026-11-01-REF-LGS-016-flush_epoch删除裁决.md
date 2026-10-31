# REF-LGS-016 · `flush_epoch` 死代码裁决：删除

> 日期：2026-11-01 ｜ 分支：`ref-lgs` ｜ 状态：**已完成（裁决 A：删除）**
> 结论：`flush_epoch` 及其「纪元变化 → 丢弃队列旧音频」分支移除；
> **零可观察行为变化**（该分支自 `7bb878b9` 起从未触发）。
> 依据：REND-016 的现场验收已把入口重同步改为「**宁糊不切**」。

---

## 1. 证据：为什么是死代码

全仓引用（删除前）只有 3 处，全是读：

| 位置 | 内容 |
|---|---|
| `start.rs:58` | `Arc::new(AtomicU64::new(0))` 创建 |
| `start.rs:412` | `Arc::clone` 给 stream owner 线程 |
| `start.rs:429-435` | 回调：纪元变化 → `sample_rx` 全部 `try_recv` + `next_block.clear()` |

**没有任何 `fetch_add` / `store`**。递增点是被 `7bb878b9`
（`fix(audio): REND-016 入口重同步改为降级保护：不切断、不跳段（宁糊不切）`）
显式删除的，同一提交把 `[RESYNC] 入口全清：淡出…+ 丢弃旧音频` 改成
`[RESYNC] 入口降级保护：不切断、不跳段，仅施加声部上限`。

当时的现场原因（提交信息与 REND-016 记录）：

1. 丢弃队列旧音频 → **可听跳跃**；
2. 懒重采样的慢块（154 / 129 ms）会**误触发**，把小爆点整段跳过，之后队列重填出现缓速；
3. 现行策略是「trim 保留最老声部、新音符被裁」，不切断、不跳段。

所以恢复触发（选项 B）等于把已被验收否决的行为加回来，不可取。

## 2. 改动

`crates/audio/gpu-synth/src/audio/playback/start.rs`（−15 / +6 行）：

- 删除 `flush_epoch` 的创建与克隆；
- 删除回调里的 `flush_seen` 与「纪元变化 → 清空 `sample_rx` / `next_block`」分支；
- 原位置保留注释：说明这里曾有该机制、为什么删除、**不要重新引入丢弃队列音频的路径**；
- 队列消费、underrun 统计、`next_block`/`next_pos` 逻辑不变。

## 3. 验证（零行为变化）

| 项 | 结果 |
|---|---|
| `cargo test -p lumino-gpu-synth --lib` | **114 passed** |
| `cargo clippy -p lumino-gpu-synth --all-targets` | 零告警 |
| `cargo fmt --check` | 通过 |
| golden（`rich.sf2` 512 filtered/unfiltered） | `ea209e1f675b4d03` / `ba3ce4034c7b5298` 不变 |
| 实时 10 s（`play_realtime`，512，逐事件） | `underruns=0`、`dropped_note_ons=0`；Governor 正常进出 L3/L2/L1/L0，`[RESYNC]` 走「降级保护」文案 |

> 说明：本卡只删死代码，音频路径本来就绕过它，所以哈希与实时指标必须与改动前一致——
> 上表正是按这个口径验收的。

## 4. 文档同步（不回写历史，只追加裁决）

- `docs/2026-10-09-REND-016-LGS实时防爆-实施记录.md`：末尾追加「REF-LGS-016 裁决」，
  并指向本节 §7（初版）与 §12（`7bb878b9` 改口径）的关系；
- `docs/2026-10-11-REND-017-GPU缓冲状态指示器-实施记录.md` §五-2：
  「是否清理由 REND-016 owner 后续裁决」→ **已裁决：删除**。

## 5. 边界

- 不恢复「过载跳段/丢音频」能力（与 REND-016 验收结论冲突）；
- 不动 Governor 四级阈值、trim/淡出策略、队列深度上限；
- 若未来确实需要「跳到最新内容」，应作为**新卡**重新设计触发条件与验收场景，
  而不是复活这个分支。
