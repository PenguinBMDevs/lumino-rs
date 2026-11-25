# AGENTS.md

Agent configuration for the lumino-rs Rust project.

> This file is written in English for tooling and review convenience. The project requirement that
> internal text (code comments, documentation, commit messages) be written in Chinese is defined
> under [Documentation](#documentation) and remains fully in force.

## Build Commands

```bash
# Build in debug mode
cargo build

# Build in release mode
cargo build --release

# Run the application
cargo run

# Build script wrappers (now under scripts/)
./scripts/build.sh [release|debug]      # Linux/macOS
scripts/build.bat [release|debug]       # Windows
python scripts/build.py [release|debug] # cross-platform
```

## Test Commands

> **Hard rule: every test must run through `cargo nextest`. `cargo test` is forbidden.**
> A bare `cargo test` bypasses nextest's per-test process isolation, timeout/retry policy and
> output conventions; the same suite can disagree between runners, and reconciling that costs far
> more than standardizing on one runner.
> If nextest is missing, install it first: `cargo install cargo-nextest --locked`
> (or `cargo binstall cargo-nextest`), then continue. **Do not fall back to `cargo test`.**
> Sole exception: **doctests** (nextest does not support them) — see the end of this section.

```bash
# Run all tests (workspace)
cargo nextest run --workspace

# Run a single test by name (substring match, equivalent to cargo test test_name)
cargo nextest run test_name

# Run tests in a specific file (test target)
cargo nextest run --test collab_id_sync_local
cargo nextest run --test collaboration_full_test
cargo nextest run --test collaboration_ui_test

# Run tests for a specific crate
cargo nextest run -p lumino-core
cargo nextest run -p lumino-gfx

# Show test output (equivalent to cargo test -- --nocapture)
cargo nextest run --no-capture

# Run specific integration tests (collaboration suites are #[ignore] by default)
cargo nextest run --test collaboration_ui_test test_collaboration_ui_all --run-ignored only
cargo nextest run --test collaboration_full_test --run-ignored only
# collab_id_sync_local requires a local lumino-server-rs listening on port 3000
cargo nextest run --test collab_id_sync_local --run-ignored only

# Run everything without stopping at the first failure
cargo nextest run --no-fail-fast

# Serialized execution (equivalent to cargo test -- --test-threads=1)
cargo nextest run -j 1

# List tests without running them (verify filter matches / that the target builds)
cargo nextest list --workspace
```

### nextest conventions (old-command cross-reference)

| Need | Forbidden | nextest equivalent |
|------|-----------|--------------------|
| Show test output | `cargo test -- --nocapture` | `cargo nextest run --no-capture` |
| Only `#[ignore]` tests | `cargo test -- --ignored` | `cargo nextest run --run-ignored only` |
| Both normal and ignored | — | `cargo nextest run --run-ignored all` |
| Serialized | `cargo test -- --test-threads=1` | `cargo nextest run -j 1` |
| Combined filtering | multiple name args | `-E 'test(/pattern/) & binary(collaboration_ui_test)'` |
| Retry failures | none | `--retries <N>` (defaults come from the profile) |

- Default config path: `workspace-root/.config/nextest.toml` (exists). It holds shared
  concurrency policy: `clipboard-serial` (`max-threads = 1`) for the tests that read/write the
  system clipboard. nextest runs every test in its own process, so an in-process `Mutex` cannot
  serialize them; concurrent clipboard tests overwrite each other (windows-latest CI failed on
  exactly this). Add new process-external resources (clipboard, global files, ports) to a serial
  test group there instead of scattering flags across the command line.
- **Doctest exception**: nextest cannot run doctests. Only when `///` code blocks in a modified
  crate need verification, `cargo test --doc -p <crate>` is allowed, and the delivery notes must
  state that this is the doctest exception.

### CI test reports (JUnit + Pages)

nextest 0.9.x no longer ships `--junit-xml`; CI consumes its libtest JSON stream and converts it:

```bash
NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 \
  cargo nextest run --workspace --message-format libtest-json-plus > nextest.ndjson
python scripts/nextest_junit.py nextest.ndjson -o junit.xml --name "nextest (local)"

# Same static dashboard that the Pages workflow publishes
python scripts/junit_report.py <dir-with-junit-xml> -o site
```

- `.github/workflows/ci.yml` uploads `junit-<os>.xml` for every platform in the test matrix.
- `.github/workflows/test-report.yml` downloads those artifacts and deploys the HTML dashboard to
  GitHub Pages at `https://<org>.github.io/<repo>/`. Pages must be switched to "GitHub Actions"
  once under repository settings.
- Test failures must still produce a report: run nextest with `--no-fail-fast`, convert/upload with
  `if: always()`, then fail the job in a final gate step.

## Lint/Format Commands

```bash
# Format code (rustfmt)
cargo fmt

# Run clippy lints
cargo clippy

# Run clippy with all targets
cargo clippy --all-targets

# Fix auto-fixable issues
cargo clippy --fix
```

## Code Style Guidelines

### Formatting
- Edition: 2024
- Max width: 100 characters
- Indent: 4 spaces (no hard tabs)
- Reorder imports: enabled

### Naming Conventions
- Crate names: `lumino-{module}` (kebab-case)
- Module directories: use flat structure (`{module}.rs` + `{module}/`)
- NO `mod.rs` files
- Types: PascalCase
- Functions/variables: snake_case
- Constants: UPPER_SNAKE_CASE

### Imports
```rust
// Group imports: std -> external -> internal
use std::{path::PathBuf, sync::Arc};
use thiserror::Error;
use tracing::{debug, error, info};
use lumino_core::{Event, Result};
```

### Error Handling
- **NEVER use `unwrap()` in production code** - all errors must be properly handled
- **NEVER use `expect()` in production code** - propagate errors with `?` instead
- **`expect()` is allowed in test code**: assertions should use `expect("...")` with context
  (never a bare `unwrap()`) so failures localize quickly. This exemption covers test code only
  (`#[cfg(test)]` modules and integration tests under `tests/`)
- **NEVER use mod.rs** - use `{module}.rs` + `{module}/`
- Use `thiserror` for custom error types
- Use `Result<T>` type alias for consistency
- Propagate errors with `?` operator
- Provide descriptive error messages

### Types
- Prefer strong types over primitive types
- Use `#[derive(Debug)]` for all public types
- Document public APIs with doc comments (`///`)
- Use `async/await` for async operations

### Testing
- **Single runner: `cargo nextest run`** — `cargo test` is forbidden (doctest exception, see
  [Test Commands](#test-commands))
- Unit tests: place in `#[cfg(test)]` module within source files
- Integration tests: place in `tests/` directory
- Use descriptive test names with `test_` prefix
- Use `tokio::test` for async tests
- Prefer `expect("context")` over `unwrap()` in assertions (see Error Handling)
- Before delivery, include the actual nextest run output; "it ran" is not evidence, the summary is

### Documentation
- **Internal text uses Chinese**: comments, documentation, code identifiers and commit messages are
  written in Chinese. This requirement is mandatory and is not relaxed by this file being English
- Follow Conventional Commits: `feat(module): description`
- Sign all commits with GPG

## DSP Read-Only Zone & AI Rules

> Source of truth: `docs/2026-11-25-DSP-AI工作规则.md` (v1.2). The body below is copied
> verbatim so agents see it without an extra hop; if the two ever disagree, the file wins
> and this section must be resynced. Human-facing rationale lives in
> `docs/2026-11-25-DSP手搓区与AI禁区.md`.
> The rule body is kept in Chinese per the project's internal-text rule; the rest of this
> file remains English for tooling convenience.

### 0. 总则

DSP 实时内核与导出限幅属于「只读区」。AI 在此区域的默认动作是**读**，不是**改**。

任何时候，只要任务需要改动只读区（哪怕只是「看起来行为不变」的重构），
必须先停下来告诉用户：为什么需要改、改哪几行、会影响什么、怎么证明没改坏。
在用户明确同意之前，不得落盘。

「不告知就改动」是本规则里唯一的一票否决项。

### 0.1 产品边界（同样不可越权）

Lumino 的音频只服务于「预览与自检」，不服务于专业制作。以下功能属于产品决策，
AI 不得把它们当作「顺手优化」自行实现，必须先由用户明确立项：

- 通用效果链（Chorus / Reverb / Delay、CC91/93/94 的实际算法）；
- 应用内 VST 宿主、插件扫描、插件 GUI 嵌入；
- 路由矩阵、总线、发送/返回、母带类处理；
- 为了「听起来更专业」而调整限幅、包络、增益结构或默认音色。

如果任务描述暗示了以上需求，AI 必须停下来指出这是产品边界问题，而不是直接写代码。
判断标准很简单：这个改动会不会让 Lumino 更像一个 DAW？会，就先问。

与 DSP 无关但同属产品边界、同样需要先问的还有：更改默认合成后端
（`SynthBackend` 的 `#[default]`）、修改工程文件格式、修改导出映射规则。

允许并鼓励的方向是「把 MIDI 更精确地交给外部」：端口/通道映射、ProgramChange 与
鼓通道语义、每端口音色库指向、CC/SysEx 透传、tempo/拍号保真。这些属于正确性，
不属于效果。

### 0.2 MIDI Clip / 激活集工作的分级

「按对象（Clip）组织 MIDI」与「按区间裁剪事件」是当前讨论中的性能方向，AI 可以参与，
但按以下分级处理：

- 激活集、冻结/离线排除、mute/solo 前移：属于 C 级，可自主实现，但必须附
  「取消静音后控制器状态重建」的测试，避免幽灵状态；
- 以 Clip 作为编辑对象：属于 B 级，需要先给出方案（CC 隔离、重叠语义、SMF 映射），
  并说明对撤销、协作、文件格式的影响；
- 按 Clip 寻址的播放/导出：属于 B 级以上，涉及播放引擎与导出管线，必须先出设计与对拍方案。

任何情况下，这条线上的工作都不得触碰下面的只读区。

### 1. 只读区（A 级）

以下路径，AI **不得修改**，只能阅读、分析、写报告：

```
crates/audio/gpu-synth/src/gpu/shaders/render.wgsl
crates/audio/gpu-synth/src/gpu/shaders/mix.wgsl
crates/audio/gpu-synth/src/gpu/shaders/mix_reduce.wgsl
crates/audio/gpu-synth/src/synth/dsp.rs
crates/audio/gpu-synth/src/synth/dsp/sinc.rs
crates/audio/gpu-synth/src/synth/dsp/filter.rs
crates/audio/gpu-synth/src/synth/dsp/envelope.rs
crates/audio/gpu-synth/src/synth/engine/limiter.rs
crates/audio/gpu-synth/src/synth/engine/voice_alloc.rs
crates/audio/gpu-synth/src/synth/engine/trim.rs
crates/audio/gpu-synth/src/synth/voices.rs
crates/audio/gpu-synth/src/audio/resample.rs
crates/tools/export/src/audio/limiter.rs
crates/audio/midi-io/src/ui_sfx.rs        # 仅 adapt_to_device / 播放回调部分
```

另外，`gpu/layout.rs` 中 `VoiceParamsStatic` / `VoiceParamsDyn` / `VoiceState`
的**字段集合与顺序**属于 A 级（改字段等于改内核契约）；纯注释修正不受限。

以上清单之外，但满足下面任一条件的改动，同样按 A 级处理：

- 会改变输出样本的浮点累加顺序或求和分组；
- 会改变包络、滤波、插值、限幅、重采样、淡出、抢占的任何参数或时序；
- 会改变块边界行为，或让输出依赖 `block_size`；
- 会改变声部生命周期（创建、释放、回收、槽位分配）的时序；
- 会修改 `STATES_SYNC_EVERY`、`LIMITER_WINDOW` 等实时契约常量；
- 会修改 xsynth fork（`PenguinBMDevs/xsynth-Lumino`）的任何代码。

### 2. 人审区（B 级）

以下区域 AI 可以起草，但必须做到：先出方案、标明影响面、附对拍计划，
改动落地后必须有人逐行复核，且必须通过逐位对拍。

```
crates/audio/gpu-synth/src/synth/engine/pool.rs
crates/audio/gpu-synth/src/synth/engine/slot.rs
crates/audio/gpu-synth/src/synth/engine/env_pool.rs
crates/audio/gpu-synth/src/synth/engine/warm.rs
crates/audio/gpu-synth/src/soundfont/*            # 解析与 zone 映射
crates/tools/export/src/audio/tick_conv.rs
crates/tools/export/src/audio/event/processor/dsp.rs
crates/tools/export/src/audio/block_scheduler.rs
```

B 级的判定标准是「会不会影响声音，但可以用确定性手段验证」。
如果 AI 无法给出可执行的对拍方案，说明这次改动应降级为 A 级处理（只出方案，不动手）。

### 3. 自主区（C 级）

以下区域 AI 可以自主修改，前提是遵守仓库通用规范（clippy、fmt、nextest、文件长度）：

- `synth/engine/{dispatch,bind,readback,render,offline,events,init,diag}.rs` 中
  不改变数学与顺序的调度/管线代码；
- `audio/playback/*`（设备流、admission、governor、drain）；
- `crates/audio/midi-io/src/api/*` 与 `playback/*` 的协议/宿主适配；
- `midi/parser.rs`、`midi/port.rs`；
- 测试、夹具、基准、文档、诊断统计；
- 纯注释修正（不改变语义的文档修正）。

C 级也不是「随便改」：任何 C 级改动如果间接影响了 A 级文件的调用方式，
必须在交付说明里列出受影响的 A 级文件清单。

### 4. 遇到 A 级需求时的固定动作

当任务确实需要改 A 级代码时，AI 必须按以下顺序执行，不得跳步：

1. **停止落盘**。可以继续读代码、跑基准、做分析，但不得修改 A 级文件。
2. **向用户报告**，内容必须包含：需要改的文件与行、为什么必须改、不改的替代方案、
   预计影响（输出是否变化、块长是否受影响、听感风险）、验证方案（用哪些素材、
   哪些哈希、跑几次 A/B）。
3. **等待明确同意**。用户说「可以」或给出替代方案之前，不做修改。
4. **落地后必须提供证据**：逐位一致的哈希、`lgs_ab.py` 结果、相关 nextest 摘要，
   以及需要人工听感抽查的具体段落。

报告模板：

```
[DSP 只读区请求]
目标：<一句话>
需要改动：<文件:行>（A 级）
为什么绕不开：<说明>
替代方案：<如果没有，写明「无」>
影响面：<输出/块长/听感/性能>
验证方案：<哈希 + A/B + 听感段落>
等待确认：是
```

### 5. 明确禁止的行为

以下行为一律视为违规，不论理由多充分：

- 不告知用户就修改只读区；
- 以「修复 bug」「格式化」「清理警告」为名顺带改动 A 级代码；
- 把 A 级改动夹带在无关 PR 里；
- 提高 `STATES_SYNC_EVERY`，或以任何方式让声部从过期状态恢复；
- 改变混音/归约的浮点累加顺序（包括引入 workgroup 树形归约、原子累加）；
- 让限幅器窗口跟随 `block_size`；
- 删除或缩短裁剪淡出、删掉每块淡出预算；
- 在音频回调中加锁、分配、日志或系统调用；
- 合并 LGS 与导出链路的两套限幅器而未做对拍；
- 在 xsynth fork 上新增 DSP 特性（只允许宿主适配类修补，且需告知）；
- 参考或复制 xsynth（LGPL-3.0）源码来实现「自研」的数据层或内核；
- 未经产品决策就实现效果链、VST 宿主、路由矩阵，或为「更专业」调整默认音色；
- 未告知用户就切换默认合成后端、改动工程文件格式或导出映射规则。

### 6. 允许的「不碰 DSP 但推进 DSP」的工作

这些是 AI 在 DSP 议题下最应该主动做的事：

- 扩充基准夹具与黄金哈希矩阵；
- 改进 `hash_render` / `lgs_ab.py` 的诊断输出；
- 为 A 级行为写**只读**的回归测试（不改内核，只加断言）；
- 数据层与控制面的实现与重构；
- 把「为什么这么做」写进文档与卡面；
- 为人工内核改动提供分相数据与性能对照表。

### 7. 与仓库规范的关系

本规则是 DSP 领域的补充约束，不替代本文件的通用规则；两者冲突时以更严格者为准。
新增或修改规则时，必须先改 `docs/2026-11-25-DSP-AI工作规则.md`，再同步本节。

## IDE Settings

### VS Code
```json
{
  "rust-analyzer.check.command": "clippy",
  "editor.formatOnSave": true,
  "editor.defaultFormatter": "rust-lang.rust-analyzer"
}
```

## CI/CD

- `master` branch: stable, release-ready
- `dev` branch: active development
- All PRs must pass clippy and tests (tests run via `cargo nextest run --workspace`)
- All commits must be GPG signed

## Terminology Convention (DEBT-21)

**The previous-generation UI layer has been removed entirely**: `Host` no longer holds any
previous-generation widget tree. Whole-window painting, hit testing and input dispatch are owned by
egui alone; the canvas and the wgpu renderer only supply textures and coordinate conversion.

Therefore, in code comments and documentation:

- **Never write 「某一层仍在接管/绘制/消费输入」** (or any equivalent): there is no second channel.
  If a piece of UI is painted by egui, name the egui module (`chrome::*`, `chrome/sidebar/*`); if it
  is painted by the canvas layer, write 「画布层」.
- When referring to the removed layer, always use **「上一代」** (on first mention you may add
  「已移除」). Do not invent aliases such as 「旧版本 / 老版本 / v1」.
- When grounding geometry or color baselines, **point to a surviving symbol** (`layout` constants,
  `PaletteSeed` fields, `Palette::TOKYO_NIGHT_STORM`) or state explicit numbers; never cite deleted
  constant names.
- Test failure messages must express the **current contract** ("the click should land on the
  canvas"), never 「否则上一代会…」.
- Compute counts with `git grep -c` and sum them; feeding `git grep -n` output to `Measure-Object`
  on a Windows console with mixed CJK text undercounts by an order of magnitude.

**Must never be renamed**: `TextLayoutMode::LegacyBox` (public enum variant, serialized into
config), `SnapSpec::from_legacy` (public API), and the `test_legacy_*` test names — historical
names, keep them byte-for-byte identical.
