# AGENTS.md

Agent configuration for the lumino-rs Rust project.

## Build Commands

```bash
# Build in debug mode
cargo build

# Build in release mode
cargo build --release

# Run the application
cargo run

# Build script wrappers
./build.sh [release|debug]    # Linux/macOS
build.bat [release|debug]     # Windows
```

## Test Commands

```bash
# Run all tests
cargo test

# Run a single test by name
cargo test test_name

# Run tests in a specific file
cargo test --test integration_test
cargo test --test collaboration_full_test
cargo test --test collaboration_ui_test

# Run tests for a specific crate
cargo test -p lumino-core
cargo test -p lumino-dms

# Run tests with output
cargo test -- --nocapture

# Run specific integration test
cargo test test_midi_to_dms_similarity --test integration_test
```

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
- **NEVER use `unwrap()`** - all errors must be properly handled
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
- Unit tests: place in `#[cfg(test)]` module within source files
- Integration tests: place in `tests/` directory
- Use descriptive test names with `test_` prefix
- Use `tokio::test` for async tests

### Documentation
- Use Chinese for comments, documentation, code identifiers and commit messages
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
- All PRs must pass clippy and tests
- All commits must be GPG signed
