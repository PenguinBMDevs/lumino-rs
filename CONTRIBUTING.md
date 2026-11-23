# CONTRIBUTING

## Project Structure

The project uses private crates for modular development, located at `crates/{domain}/{module}`, divided into 5 groups by functional domain:

- `core/` — Foundational data and infrastructure (core, diagnostics, message, project)
- `audio/` — MIDI / audio / playback (gpu-synth, midi-io, midi-loader, midi-model, midiplayer, note-core)
- `editor/` — UI / rendering / editing (dialog, editor-state, gfx, ui, ui-core, ui-editor, ui-settings)
- `network/` — Collaboration / cloud (cloud, collaboration)
- `tools/` — Tools / export (export, extras)

- For the module's directory, it should be named after its role in the project, e.g.: `core`
- For the module's crate name, the project name should be appended, e.g.: `lumino-core`
- Naming must follow `kebab-case`, all lowercase

## Code Rules

- The use of `unwrap()` is prohibited; all errors must be properly handled
- `{module}/mod.rs` should not be used; use `{module}.rs` + `{module}/` instead

## Commit Convention

- Commit messages must follow the [Conventional Commits 1.0](https://www.conventionalcommits.org/en/v1.0.0/) standard
  - The scope of changes must be specified, e.g. `feat(midi):`
  - Breaking changes must use `[type]!`
- All commits must be GPG-signed
- Before committing, please run `cargo clippy` to check code quality and `cargo fmt` to format the code
