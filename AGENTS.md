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

## Size & Structure Discipline (DEBT-10)

> Enforced by review, not by CI. There is deliberately no file-size job: the old 400-line guard was
> deleted in `f740d1d6`, and DEBT-10 decided not to add a replacement. `clippy.toml` only constrains
> function length once `too_many_lines` is enabled, so treating it as a file-size rule is wrong.

| Object | Hard limit | Soft target | Notes |
|---|---:|---:|---|
| Production `.rs` file | **600** | 500 | level-A DSP files are reported, never force-split |
| Test `.rs` file | **800** | 600 | one `#[test]` function ≤ 40 lines |
| Function | **100** | 80 | generated i18n tables are data, not hand-written functions |
| `struct` fields | **20** | 12 | a pure DTO needs a comment saying why it is that wide |
| `impl` methods | **25** | 15 | `Default` / `From` impls do not count |
| Function parameters | **6** | 4 | beyond that, use a named parameter struct |
| Module directory | **4,000 lines / 20 files** | 2,500 / 12 | beyond that, register an owner and a split plan |

Split by responsibility, never by line count:

- Do not create a file to satisfy a number. A 4-line module holding one comment is not a split.
- A split must move state with the behaviour. Extracting more `impl` blocks of the same `struct`
  does not reduce coupling; `chrome.rs` (DEBT-10/T-10) is the reference: phases, channels and
  overlay ordering became separate files, and `show_bands` dropped from 539 to 8 lines.
- When a file crosses 600 lines, split along a phase or layer boundary and write down the order
  that must be preserved (draw order, call order, upload order).
- No new `#[allow(clippy::too_many_lines)]`; an existing one must carry `TODO(DEBT-10/T-xx)` and a
  removal plan. Enabling the lint workspace-wide before the baseline is burned down is forbidden:
  CI runs `-D warnings`, so it would fail on 309 functions at once.
- The full task list, baselines and burn-down tables live in
  `docs/2026-11-26-DEBT-10-结构减法与尺寸治理.md`.

## DSP Read-Only Zone & AI Rules

> **Status: authoritative and self-contained (v1.4, 2026-11-25).** This section is the single
> source of truth for DSP work and applies to every AI collaborator, including subagents. It
> absorbed the former rule document and its rationale document under `docs/`; those files are kept
> as background reading only and must not be treated as a second source of truth. This section
> supplements the rest of this file: where the two conflict, the stricter rule wins.

### 0. General

The DSP real-time kernels and the export limiter are a **read-only zone**. For this zone an AI
agent's default action is to **read**, never to **write**.

Whenever a task requires touching the read-only zone — including "behavior-preserving" refactors —
the agent must stop and tell the user why the change is needed, which lines are affected, what the
impact is, and how to prove nothing broke. Nothing may be written before the user explicitly
agrees.

"Changing it without telling the user" is the single hard veto in these rules.

**Entry gate.** Before the first edit of any task that touches `crates/audio/gpu-synth`,
`crates/tools/export/src/audio`, `crates/audio/midi-io` audio behavior, or playback/export audio
behavior, state which level (A / B / C) the change belongs to. A task that cannot be classified is
level A until the user says otherwise.

Where "DSP" actually lives — the level of a change is decided by these three concentric circles,
not by the directory name:

- **Inner circle: the LGS real-time kernel.** Three WGSL shaders (`render.wgsl`, `mix.wgsl`,
  `mix_reduce.wgsl`) plus the Rust-side coefficients and state that feed them: `synth/dsp/sinc.rs`
  builds the windowed-sinc tables, `filter.rs` computes RBJ biquad coefficients, `envelope.rs`
  describes the seven-segment envelope, `voices.rs` assembles SF2 zones into GPU parameters. This
  circle decides what every sample looks like.
- **Middle circle: the timing and gain trajectory that lets the inner circle run correctly.**
  `engine/limiter.rs` (look-ahead limiting), `audio/resample.rs` (device sample-rate conversion),
  `engine/voice_alloc.rs` (allocation and stealing), `engine/trim.rs` (polyphony trimming and
  fades), `engine/cc.rs` (CC-to-DSP mapping). It does not write samples, but it decides when, at
  what gain and under which envelope they are computed. It is an audible boundary too: one mistake
  is a click, a cut note or a volume jump.
- **Outer circle: code outside this repository.** `xsynth-core` (CPU kernel used for realtime
  fallback and CPU offline export), `xsynth-realtime` (its realtime driver) and
  `xsynth-soundfonts` (SF2 parsing, also used by LGS) all come from the fork
  `PenguinBMDevs/xsynth-Lumino` (LGPL-3.0, isolated in its own repository and pinned to a fixed rev
  through the workspace's `[patch.crates-io]`). Beyond that sits Maestro, an externally loaded
  synth whose DSP is not ours at all.

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

MIDI itself does not carry effects: CC91/93/94 are *send amounts* (reverb, chorus, delay), and the
standard leaves the algorithms to the device. Implementing a reverb inside the built-in synth
therefore defines Lumino's sound — a synth feature, not MIDI compliance. The LGS backend handles
CC7/10/11, CC64, CC71/72/73 and NRPN in `synth/engine/cc.rs` but ignores 91/93/94, while KDMAPI,
system MIDI and Maestro forward the bytes unchanged, so the professional path already exists. The
built-in backend ignoring these send amounts must be stated in the settings page or the
documentation, so that users route spatial effects to an external synth or a DAW instead of filing
it as a bug.

Changes that are not DSP but still belong to the product boundary and also need approval:
changing the default synth backend (`SynthBackend`'s `#[default]`), changing the project file
format, or changing export mapping rules.

Updating the xsynth fork (dependency rev bump or upstream merge) is a separate and currently
deferred decision: do not bump it as part of unrelated work, and do not pull its performance
backlog (PREF items) forward until the CPU backend's role is re-decided.

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

The performance gain comes from indexing and culling, not from the name: per-time-range indexes,
iterating only the ranges a block covers, and excluding frozen/offline content at load time all
work on a track-plus-range model. Hence the order: activation sets first (level C), clips as
editing objects second (level B), clip-addressed playback/export third (level B or stricter).
Clip-as-object work has to answer three questions before any model migration: how CC isolation is
achieved (a channel per clip, or per-clip controller state, which changes synth and playback
semantics), how overlapping clips order events and merge CC, and how SMF import/export maps objects
back to tracks. Activation sets additionally need the ghost-state fix above: events skipped while
muted leave the channel's controller state stale, so unmute must rebuild it explicitly.

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

**Adjudicated engine files (DEBT-10/T-06, 2026-11-26).** Two engine files were previously absent from
the A/B/C lists and therefore defaulted to level A without an explicit boundary. They are now
classified, and the classification never relaxes the blanket clause below:

- `crates/audio/gpu-synth/src/synth/engine/cc.rs` — **level A** for any change to a mapped value or
  its timing. This is the middle-circle "CC-to-DSP mapping" from §0: CC7/10/11 change gain and pan,
  CC71/72/73 change filter parameters, RPN/NRPN change pitch. Level C is allowed only for
  comment-only fixes, log text, or a rename that provably does not change the computed value.
- `crates/audio/gpu-synth/src/synth/engine.rs` (the facade; the path is not `engine/engine.rs`) —
  **level A** for anything touching dispatch order, block slicing, bucket construction, state
  ownership or voice lifecycle. Level C is limited to the same kind of scheduling plumbing as §3
  (`dispatch` / `bind` / `render`), and only when it provably neither reorders calls nor changes math.

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
- trading exact math for speed (lower-order sinc interpolation, biquad replaced by a single-pole
  approximation) without spectrum comparison and human listening;
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

### 7. Acceptance model

Three conditions must hold at the same time: **bit-exact**, **correct by ear**, and
**block-size independent**. If one fails, the change is not done, no matter how green CI is.

Bit-exactness is not fastidiousness, it is the only automatable acceptance. `synth/dsp.rs` mirrors
the XSynth formulas precisely so that LGS output can be compared sample by sample against XSynth
and against the reference audio used for acceptance. The seven envelope segments, the convex `f^8`
curve, the concave `(1-f)^8` curve, and "a segment starts from the last output value of the
previous segment" all exist to make the two backends produce the same waveform from the same MIDI.
A change that looks harmless — say, restarting a segment from zero — shifts the whole output by a
tiny amount; a test that only compares RMS or length lets it through, the ear hears a thin layer
of grit in dense passages, and the only automated oracle is gone, so every later performance
number becomes an orphan.

### 8. Why these specific rules exist

**8.1 Parallelism must freeze the order before it adds threads.** `mix.wgsl` records the history:
the old kernel had one thread walk every voice, giving 4 workgroups for block 512 on a 5080 and
about 11.5 ms per block. The new kernel does not simply split the sum: the CPU builds stable
buckets by channel and released flag, keeps the original ascending voice order inside a bucket,
assigns one thread per (frame, channel) pair accumulating in that order, and finishes with a small
reduction pass in ascending channel order. `mix_reduce.wgsl` says the same: the reduction must be
in ascending channel order because that was the order of the old single-threaded kernel. Float
addition is not associative, so "how to parallelize" is a data-structure question, not a thread
count question.

**8.2 The limiter window must be decoupled from the render block.** `LIMITER_WINDOW = 512` exists
because the gain trajectory may depend only on the signal and the absolute frame position, never
on how the render block is sliced: look-ahead limiting rewinds "future" samples at the end of the
window to compute gain from the whole-window peak. Measured in REF-LGS-008: with limiting enabled,
block 512 versus 2048 first diverges at frame 304, with 96% of samples differing. Block-size
independence is what allowed REF-LGS-009 to raise the default offline export block size from 512
to 2048 — it is the precondition for touching block size at all.

**8.3 State readback cannot be throttled, because that is replay.** `STATES_SYNC_EVERY` is 1 in
`synth/engine/types.rs`: finished voices are reclaimed only after readback, so raising it makes
voices resume from stale state and replays audio — explicitly forbidden. It is not a tunable knob
but the external form of "who owns state": the CPU is the authoritative copy, the GPU hands back
advanced state every block, and the CPU merges it. Removing that round trip requires making the GPU
own state, which is a structural change, not 1 → 4.

**8.4 The static/dynamic table boundary is "does it change", not "is it big".** `gpu/layout.rs`
splits voice parameters into a 64-byte static table uploaded once at slot allocation and a 52-byte
dynamic table uploaded every block, with allocation, reclamation and dirty-range merging in
`pool.rs` / `slot.rs`. REF-LGS-010 cut per-voice per-block upload from 250 to 120 bytes (52 dynamic
+ 68 state), worth 10–20% wall clock in A/B. The rule behind it: every field rewritten during the
voice lifetime must stay dynamic — `speed` changes with pitch bend, `sample_offset` resolves only
after the sample upload, `env_*` is rewritten by envelope phase and fades, `release_at` changes on
NoteOff. Moving one to the static table means a dirty mark at every write site and a wrong note
wherever one is missed. The REF-LGS-010 card assumed `sample_offset*` and `speed` were static; the
implementation kept them dynamic because measurement disagreed.

**8.5 Resampling must cross blocks, because truncation is aliasing.** `audio/resample.rs` is a
polyphase FIR with the cutoff at 0.9 × output Nyquist, causally one block behind: each `process`
emits the previous input block while the current block only supplies the "future" taps. Otherwise
the filter is truncated at every block boundary, high frequencies fold back into the audible band,
and dense high-frequency material crackles — a symptom unrelated to render cost, which misleads
diagnosis.

**8.6 Trim fades are 5 ms, not 1 ms.** `FADE_OUT_MS` records a real listening regression: 1 ms
fades stacked into a "chainsaw" under dense trimming, so it became a 5 ms linear decay, plus a
per-block budget (`MAX_TRIM_FADES_PER_BLOCK`) that spreads same-block hard cuts over several
blocks. Intuition says a shorter fade is less audible; when fades stack, the opposite is true.

### 9. Anti-patterns that look like optimizations

Each of these is plausible enough that someone will try it unless it is written down:

1. Replacing ordered accumulation with a parallel reduction. The speedup is real, the bit-exactness
   is gone, and with it the right to compare against XSynth.
2. Raising `STATES_SYNC_EVERY` to save a map. It saves microseconds and replays note tails, only
   intermittently and only in dense passages — nearly impossible to reproduce.
3. Letting the limiter window follow the block size. Block size is a user setting, so the output
   would depend on export settings and two exports of one project could differ.
4. Locking or allocating in the audio callback. `ui_sfx.rs` fixes the discipline: zero locks, zero
   allocations, read two atomics and move samples. Priority inversion on the realtime thread shows
   up as dropouts, which tests normally never see.
5. Deleting or shortening fades for performance. See 8.6: audibly worse.
6. "Unifying" the two limiters. LGS is look-ahead peak limiting; the export chain is LoudMax-style
   compression/limiting with its own threshold, attack and release. They are different algorithms,
   so merging changes both chains at once. The correct order is parity comparison, quantified
   difference, decision — then delete one.
7. Adding an effects chain to the xsynth fork. Its patch surface is audio-device pacing and
   output-stream ownership, i.e. the adaptation needed to run inside our host; adding effects turns
   an external dependency into a second in-house DSP, the hardest kind to maintain — rebasing
   against upstream while validating sound by ear.
8. Trading approximate math for speed (lower-order sinc interpolation, biquad to single-pole). The
   waveform looks fine, the spectrum does not, and the ear hears "muddy" or "harsh". Without a
   spectrum comparison and human listening, do not touch it.

### 10. Division of labor

**10.1 Why the realtime kernel is not delegated.** Not because an agent cannot write DSP, but
because the failure modes of this code match what an agent verifies worst. Acceptance depends on
bit-exactness, listening, and timing behavior on real hardware: bit-exactness only proves "same as
the reference", not "sounds right"; listening needs a human and specific material (dense black-MIDI,
long tails, single-port material); timing depends on drivers, buffers and system load and does not
reproduce in a test environment. REND-015's 1 ms → 5 ms fade is the model case — numerically
"correct", audibly a chainsaw. The symptoms here are intermittent clicks, vanishing tails, "only on
one soundfont": expensive to trace and easy to blame on the user's environment. Handing that to a
collaborator who cannot stay, cannot listen, and cannot load the target hardware transfers risk to
future maintainers.

**10.2 The three high-value things an agent can do.**

- **Oracles.** `hash_render`, `lgs_ab.py`, the golden-hash matrix, phase statistics — the only
  lever that turns human kernel work into something regressible. REF-LGS-013 laid the foundation;
  fixture coverage, failure diagnostics and CI orchestration still have room.
- **The data layer.** SF2 parsing, zone tables, sample packing, caching and lifetime. Not realtime,
  deterministic oracle, and both backends benefit. This is the only part of "removing the
  dependency" that can be engineered forward.
- **The control plane.** Event routing, port mapping, ProgramChange dispatch, SysEx interpretation,
  config and UI, statistics and diagnostics. Protocol, not math: a mistake is a functional bug, not
  a sound-quality bug, and rolls back cheaply.

**10.3 A workable collaboration loop.**

1. Write the spec first, not the code — e.g. "how should CC91 depth map to a parameter", "how does
   ProgramChange route when one SF2 holds several presets". Cross-backend semantics are confirmed
   by a human and drafted by an agent.
2. Implement: human for level A, agent for level C, agent drafts plus line-by-line human review for
   level B.
3. Acceptance always includes three things: golden-hash bit-exactness, the `lgs_ab.py` 2% gate, and
   a human listening spot check on real material. Missing one means not done.
4. Record the reason, the impact surface and the parity data in the card and the docs, so the next
   person knows which assumptions were verified.

### 11. Cost of crossing the boundary

Modifying the realtime kernel without telling anyone: at best bit-exactness is lost and every
performance number loses its reference; at worst a soundfont develops intermittent clicks or cut
notes, and diagnosis drifts toward rendering, drivers or the user's device; in the worst case a
lock or allocation reaches the realtime thread, producing sustained dropouts on low-end machines
that never reproduce on a development box. Copying xsynth's implementation into a "home-grown"
parser is not a technical problem but a licensing one: the isolation boundary fails and the whole
project's license structure has to be re-evaluated.

Staying inside the boundary compounds instead: oracles make optimization regressible, the data
layer makes both backends share one parser, and the control plane concentrates semantic questions
in one place. None of it shows up in frame rate immediately, and all of it makes the next change
cheaper.

### 12. One-line principle

**Humans own "does it sound right" and "what is the product"; agents own "how do we prove it stays
right".** Anything that both affects sound and cannot be proven automatically belongs to the
read-only zone; anything that changes the product's identity (effects chain, VST host, routing
matrix) needs a human decision rather than an incidental improvement.

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

- **Never claim that a layer still owns, paints or consumes input** (or any equivalent): there is no
  second channel. If a piece of UI is painted by egui, name the egui module (`chrome::*`,
  `chrome/sidebar/*`); if it is painted by the canvas layer, say "the canvas layer".
- When referring to the removed layer, always use **"the previous generation"** (on first mention
  you may add "removed"). Do not invent aliases such as "old version", "legacy version" or "v1".
- When grounding geometry or color baselines, **point to a surviving symbol** (`layout` constants,
  `PaletteSeed` fields, `Palette::TOKYO_NIGHT_STORM`) or state explicit numbers; never cite deleted
  constant names.
- Test failure messages must express the **current contract** ("the click should land on the
  canvas"), never a hypothetical of the form "otherwise the previous generation would…".
- Compute counts with `git grep -c` and sum them; feeding `git grep -n` output to `Measure-Object`
  on a Windows console with mixed CJK text undercounts by an order of magnitude.

The governed comments are written in Chinese, so apply these rules to their Chinese wording: use
the direct Chinese equivalent of each canonical term above, keep it consistent, and never introduce
a second alias.

**Must never be renamed**: `TextLayoutMode::LegacyBox` (public enum variant, serialized into
config), `SnapSpec::from_legacy` (public API), and the `test_legacy_*` test names — historical
names, keep them byte-for-byte identical.
