# Issue 2 · turtle261/zpaq-rs — global advapi32 link declaration masks downstream link defects

## Title

```
docs: build.rs `cargo:rustc-link-lib=advapi32` propagates to the whole dependency tree — record the global impact (real-world case study)
```

## Body

### Summary

This is not a bug in `zpaq_rs` itself — the `advapi32` declaration in `build.rs` is
required for the vendored `zpaq.cpp` CryptoAPI usage. However, cargo's link-lib
propagation semantics make this declaration **global for every final link target in the
dependency tree**, which means `zpaq_rs` can silently mask missing system-library
declarations in *other* crates. We just hit a real-world chain reaction and would like it
documented so other users can debug similar cases.

### Mechanism

Cargo forwards every `cargo:rustc-link-lib` instruction emitted by a build script to the
linker invocation of **any** binary/test/example that transitively depends on that crate.
There is no scoping mechanism ("private" link declarations do not exist). Consequently:

> If your dependency tree contains `zpaq_rs`, every final binary on Windows links
> `advapi32` — whether or not it needs it, and regardless of which crate needed it.

### Real-world chain reaction (case study, 2026-08)

1. [`unrar_sys`](https://github.com/muja/unrar.rs) vendored unrar C++ code uses
   `advapi32` APIs (`CryptAcquireContextW`, `CheckTokenMembership`, `SetFileSecurityW`,
   …) but its `build.rs` never declared the library.
2. Until Rust 1.87.0 (2025-05-15), rustc linked `advapi32` by default into `std`, hiding
   the defect. [rust-lang/rust#138233](https://github.com/rust-lang/rust/pull/138233)
   removed that default; the official release notes explicitly say C libraries relying on
   that assumption "may need to explicitly link advapi32".
3. A downstream project (Lumino) depended on both `unrar_sys` (via `unarc-rs`) and
   `zpaq_rs`. After the rustc change, the missing `advapi32` declaration was **masked by
   zpaq_rs' global declaration** — builds kept passing.
4. The project then removed `zpaq_rs` (for unrelated reasons). The very next `cargo test`
   failed with 13 `LNK2019` unresolved `advapi32` symbols from `libunrar_sys`.
   Removing a crate whose declaration was *correct* broke the build of an *unrelated*
   crate — a textbook case of a hidden cross-crate dependency.

Full controlled reproduction (two-crate workspace: `unrar-minimal` → link fails,
`unrar-with-zpaq` → link passes; same unrar call site, only the `zpaq_rs` dependency
differs) ships with the parallel upstream issue.

### Suggested action (docs only)

Since cargo offers no scoped-link mechanism, the fix belongs upstream in `unrar_sys`
(issue filed there). For this repo, we suggest:

1. Add a short note to `README.md` (Platform notes section) stating that the Windows
   `advapi32` link declaration propagates to every final link target in the dependency
   tree, and that downstream crates must not rely on it to satisfy their own Windows
   system-library requirements.
2. Optionally mention in `build.rs` comments why the declaration exists (CryptoAPI usage
   in `zpaq.cpp`), so future maintainers don't "simplify" it away and silently break
   downstream trees that came to depend on the side effect.

### Related

- upstream defect being masked: `muja/unrar.rs` — `unrar_sys` missing `advapi32`
  declaration (issue filed there in parallel)
- rustc change that started the chain: [rust-lang/rust#138233](https://github.com/rust-lang/rust/pull/138233)

---
