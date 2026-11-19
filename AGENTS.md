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

- Default config path: `workspace-root/.config/nextest.toml`. The repository does not have one yet;
  add it when shared timeout/retry/concurrency policy is needed instead of scattering flags across
  the command line.
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
