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

> Source of truth: `docs/2026-11-25-DSP-AI工作规则.md` (v1.3). This section is the English
> rendering of that document; if the two disagree, the Chinese document wins and this
> section must be resynced. Human-facing rationale: `docs/2026-11-25-DSP手搓区与AI禁区.md`.
> Sync order: edit the document first, then this section.

### 0. General

The DSP real-time kernels and the export limiter are a **read-only zone**. For this zone an
AI agent's default action is to **read**, never to **write**.

Whenever a task requires touching the read-only zone — including "behavior-preserving"
refactors — the agent must stop and tell the user why the change is needed, which lines are
affected, what the impact is, and how to prove nothing broke. Nothing may be written before
the user explicitly agrees.

"Changing it without telling the user" is the single hard veto in these rules.

### 0.1 Product boundary (also non-negotiable)

Lumino's built-in audio is for preview and self-check only, not for production. The following
are product decisions; an agent must not implement them as "incidental improvements":

- a general effects chain (chorus / reverb / delay, the actual algorithms behind CC91/93/94);
- an in-app VST host, plugin scanning, or embedded plugin GUIs;
- a routing matrix, buses, sends/returns, or mastering-grade processing;
- changing limiters, envelopes, gain structure, or default timbre "to sound more professional".

If a task implies any of the above, the agent must stop and flag it as a product-boundary
question instead of writing code. The test is simple: would this make Lumino more like a DAW?
If yes, ask first.

Changes that are not DSP but still belong to the product boundary and also need approval:
changing the default synth backend (`SynthBackend`'s `#[default]`), changing the project file
format, or changing export mapping rules.

The encouraged direction is "hand MIDI to external tools more precisely": port/channel
mapping, ProgramChange and drum-channel semantics, per-port soundfont targeting, CC/SysEx
pass-through, tempo/time-signature fidelity. That is correctness, not effects.

### 0.2 Level of MIDI Clip / activation-set work

"Organizing MIDI by object (clips)" and "culling events by time range" are the performance
directions under discussion. Agents may participate, at these levels:

- activation sets, freeze/offline exclusion, moving mute/solo earlier: level C, may be
  implemented autonomously, but must include a test for "rebuilding controller state after
  unmute" to avoid ghost state;
- clips as editing objects: level B, requires a plan first (CC isolation, overlap semantics,
  SMF mapping) and an impact statement for undo, collaboration and the project file format;
- clip-addressed playback/export: level B or stricter; touches the playback engine and export
  pipeline, so a design and a parity plan are mandatory before any code.

None of this work may touch the read-only zone below.

### 1. Read-only zone (level A)

The agent must **not modify** the following paths; reading, analysis and reports are fine:

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
crates/audio/midi-io/src/ui_sfx.rs        # adapt_to_device / audio-callback parts only
```

In addition, the **field set and order** of `VoiceParamsStatic` / `VoiceParamsDyn` /
`VoiceState` in `gpu/layout.rs` are level A (changing fields changes the kernel contract);
comment-only fixes are not restricted.

Any change outside the list above that meets any of the following is also level A:

- changes the floating-point accumulation order or grouping of the output;
- changes any parameter or timing of envelopes, filters, interpolation, limiting,
  resampling, fades or voice stealing;
- changes block-boundary behavior or makes output depend on `block_size`;
- changes voice lifecycle timing (spawn, release, reclamation, slot allocation);
- changes real-time contract constants such as `STATES_SYNC_EVERY` or `LIMITER_WINDOW`;
- changes any code in the xsynth fork (`PenguinBMDevs/xsynth-Lumino`).

### 2. Human-review zone (level B)

Agents may draft changes here, but must: present a plan, state the impact, attach a parity
plan; the change must then be reviewed line by line and pass bit-exact comparison.

```
crates/audio/gpu-synth/src/synth/engine/pool.rs
crates/audio/gpu-synth/src/synth/engine/slot.rs
crates/audio/gpu-synth/src/synth/engine/env_pool.rs
crates/audio/gpu-synth/src/synth/engine/warm.rs
crates/audio/gpu-synth/src/soundfont/*            # parsing and zone mapping
crates/tools/export/src/audio/tick_conv.rs
crates/tools/export/src/audio/event/processor/dsp.rs
crates/tools/export/src/audio/block_scheduler.rs
```

Level B means "affects sound, but can be verified deterministically". If the agent cannot
produce an executable parity plan, treat the change as level A (plan only, do not touch).

### 3. Autonomous zone (level C)

Agents may modify these autonomously, subject to the repository rules (clippy, fmt, nextest,
file length):

- scheduling/pipeline code in
  `synth/engine/{dispatch,bind,readback,render,offline,events,init,diag}.rs`
  that does not change math or ordering;
- `audio/playback/*` (device streams, admission, governor, drain);
- protocol/host adaptation in `crates/audio/midi-io/src/api/*` and `playback/*`;
- `midi/parser.rs`, `midi/port.rs`;
- tests, fixtures, benchmarks, docs, diagnostics;
- comment-only fixes.

Level C is still not "anything goes": if a level-C change indirectly changes how a level-A
file is called, the delivery notes must list the affected level-A files.

### 4. Fixed procedure when a level-A change is needed

1. **Stop writing.** Reading code, running benchmarks and analysis are fine; modifying
   level-A files is not.
2. **Report to the user**, including: files and lines, why it cannot be avoided, alternatives,
   expected impact (output, block size, listening risk), verification plan (materials,
   hashes, A/B runs).
3. **Wait for explicit approval.** Do not modify anything before the user says yes or
   proposes an alternative.
4. **Provide evidence afterwards**: bit-exact hashes, `lgs_ab.py` results, relevant nextest
   summaries, and the exact passages that need human listening checks.

Report template:

```
[DSP read-only request]
Goal: <one line>
Files to change: <file:line> (level A)
Why it cannot be avoided: <explanation>
Alternatives: <or "none">
Impact: <output / block size / listening / performance>
Verification: <hashes + A/B + listening passages>
Awaiting approval: yes
```

### 5. Explicitly forbidden

- modifying the read-only zone without telling the user;
- smuggling level-A edits in as "bug fixes", "formatting" or "warning cleanup";
- bundling level-A edits into an unrelated PR;
- raising `STATES_SYNC_EVERY`, or letting voices resume from stale state in any way;
- changing the floating-point accumulation order of mixing/reduction (including workgroup
  tree reductions or atomics);
- letting the limiter window follow `block_size`;
- removing or shortening trim fades, or removing the per-block fade budget;
- locking, allocating, logging or making system calls in the audio callback;
- merging the two limiters (LGS and export) without a parity comparison;
- adding DSP features to the xsynth fork (host-adaptation patches only, and only with notice);
- referencing or copying xsynth (LGPL-3.0) source to build a "home-grown" data layer or kernel;
- implementing an effects chain, VST host or routing matrix without a product decision, or
  tweaking default timbre "to sound more professional";
- switching the default synth backend, changing the project file format or changing export
  mapping rules without telling the user.

### 6. Work that advances DSP without touching it

- expanding benchmark fixtures and the golden-hash matrix;
- improving `hash_render` / `lgs_ab.py` diagnostics;
- writing **read-only** regression tests for level-A behavior (assertions only, no kernel edits);
- implementing and refactoring the data layer and the control plane;
- writing down "why it is done this way" in docs and cards;
- producing phase data and performance comparisons for human kernel work.

### 7. Relation to repository rules

These rules supplement, not replace, the rest of this file; when they conflict, the stricter
rule wins. When changing them, edit `docs/2026-11-25-DSP-AI工作规则.md` first, then resync
this section.

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
