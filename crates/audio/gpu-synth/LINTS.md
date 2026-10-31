# `lumino-gpu-synth` lint 口径（REF-LGS-018）

> 本文件记录本 crate 继承工作区 lint 后的**白名单项与理由**。
> 规则来源：仓库根 `Cargo.toml` 的 `[workspace.lints.clippy]` + `AGENTS.md`。

## 1. 继承了哪些规则

`crates/audio/gpu-synth/Cargo.toml` 现在声明：

```toml
[lints]
workspace = true
```

即继承根 `Cargo.toml` 的：

| lint | 级别 | 含义 |
|---|---|---|
| `clippy::mod_module_files` | deny | 禁止 `mod.rs`，用 `{module}.rs` + `{module}/` |
| `clippy::unwrap_used` | deny | 生产与测试代码都禁止 `Option::unwrap` / `Result::unwrap` |

`expect_used` 未全局 deny（`AGENTS.md`：测试代码允许带上下文的 `expect`），
生产代码的 `expect` 仍由 `AGENTS.md` + review 守住。

### 生效性验证（不是"配了就完事"）

2026-10-31 用一个临时 `examples/lint_probe.rs`（内含 `Option::unwrap()`）验证：

```
crates/audio/gpu-synth/examples/lint_probe.rs:6:13: error: used `unwrap()` on an `Option` value
error: could not compile `lumino-gpu-synth` (example "lint_probe")
```

探针删除后 `cargo clippy -p lumino-gpu-synth --all-targets` 零告警。
说明规则确实作用到 **lib / tests / examples / benches** 全部目标，而非只作用于 lib。

## 2. 白名单（合理保留项）

| 位置 | 形式 | 为什么保留 |
|---|---|---|
| `src/audio/playback/admission.rs:103,137` | `lock().unwrap_or_else(\|e\| e.into_inner())` | **锁中毒恢复**：令牌桶只是限流计数，中毒后继续用旧值比 panic/丢流更安全。`unwrap_or_else` 不是 `unwrap_used` 的检查对象，此处记录以防后续被误改。 |
| `#[cfg(test)]` 模块（`drain/tests.rs`、`soundfont/tests.rs`、`dsp/tests.rs`、`gpu/context.rs`） | `.expect("上下文")` | `AGENTS.md` 明确豁免测试代码；`expect` 带上下文，失败可直接定位。 |
| `examples/hash_render.rs`、`examples/profile_gpu.rs` | `.expect("...")` | 命令行基准工具：参数/加载失败应立即失败并给出可读信息，返回 `Result` 只会把错误藏进 `main` 的样板代码。**不属于生产路径**（不随库发布）。 |
| 文档示例（`src/audio/wav.rs:211-213`、`src/midi/parser.rs:20`、`src/soundfont/types.rs:79`） | `/// ... .unwrap()` | doctest 面向使用者，展示最简用法；clippy 不对 doc 代码跑 `unwrap_used`，且 doctest 本身会验证可用性。 |

## 3. 结论

- **生产代码零 `.unwrap()`**（`src/**` 非测试模块，grep + clippy 双证）；
- 无 `mod.rs`；
- 不需要任何 crate 级 `#![allow(...)]`，也不需要行级 `#[allow(clippy::unwrap_used)]`。

## 4. 复查命令

```bash
cargo clippy -p lumino-gpu-synth --all-targets
cargo fmt -p lumino-gpu-synth -- --check
cargo test -p lumino-gpu-synth --lib
```
