# Issue 1 · muja/unrar.rs — unrar_sys missing advapi32 link declaration on Windows

## Title

```
unrar_sys: LNK2019 unresolved advapi32 symbols (CryptoAPI / token / ACL / registry) on Windows with Rust >= 1.87
```

## Body

### Summary

`unrar_sys` fails to link on Windows (MSVC) with Rust >= 1.87 when no other crate in the
dependency tree happens to declare `advapi32`. The vendored unrar C++ sources call many
`advapi32` APIs, but `unrar_sys/build.rs` never declares that library — it declares
`shell32` and `powrprof` (`cargo:rustc-flags=-lpowrprof`) but **not `advapi32`**. This
used to be masked by rustc's default linking of `advapi32` into `std`, which was removed
in Rust 1.87.0 — and the official release notes explicitly put the responsibility on
C library bindings:

> **Compatibility Notes** — [Rust 1.87.0](https://github.com/rust-lang/rust/releases/tag/1.87.0):
> "Windows: The standard library no longer links `advapi32`, except on win7.
> **Code such as C libraries that were relying on this assumption may need to explicitly
> link advapi32.**"
> ([rust-lang/rust#138233](https://github.com/rust-lang/rust/pull/138233))

### Environment

- OS: Windows 10/11 x64, MSVC toolchain
- Rust: 1.87.0+ (reproduced with 1.97.1)
- `unrar_sys` 0.5.8 (latest published; master as of 2026-08 is still affected)
- Full repro workspace: `unrar-minimal` (fails) vs `unrar-with-zpaq` (passes) — same
  unrar call site, only a `zpaq_rs` dependency differs (ships with this issue)

### Error (13 unresolved externals, all `advapi32`)

```
libunrar_sys-*.rlib(pathfn.o)   : error LNK2019: unresolved external symbol __imp_RegCloseKey referenced in GetRarDataPath
libunrar_sys-*.rlib(pathfn.o)   : error LNK2019: unresolved external symbol __imp_RegOpenKeyExW referenced in GetRarDataPath
libunrar_sys-*.rlib(pathfn.o)   : error LNK2019: unresolved external symbol __imp_RegQueryValueExW referenced in GetRarDataPath
libunrar_sys-*.rlib(system.o)   : error LNK2019: unresolved external symbol __imp_OpenProcessToken referenced in Shutdown / ExtractACL20
libunrar_sys-*.rlib(system.o)   : error LNK2019: unresolved external symbol __imp_AdjustTokenPrivileges referenced in Shutdown / ExtractACL20
libunrar_sys-*.rlib(system.o)   : error LNK2019: unresolved external symbol __imp_AllocateAndInitializeSid referenced in IsUserAdmin
libunrar_sys-*.rlib(system.o)   : error LNK2019: unresolved external symbol __imp_CheckTokenMembership referenced in IsUserAdmin
libunrar_sys-*.rlib(system.o)   : error LNK2019: unresolved external symbol __imp_FreeSid referenced in IsUserAdmin
libunrar_sys-*.rlib(system.o)   : error LNK2019: unresolved external symbol __imp_LookupPrivilegeValueW referenced in Shutdown / ExtractACL20
libunrar_sys-*.rlib(crypt.o)    : error LNK2019: unresolved external symbol __imp_CryptAcquireContextW referenced in GetRnd
libunrar_sys-*.rlib(crypt.o)    : error LNK2019: unresolved external symbol __imp_CryptReleaseContext referenced in GetRnd
libunrar_sys-*.rlib(crypt.o)    : error LNK2019: unresolved external symbol __imp_CryptGenRandom referenced in GetRnd
libunrar_sys-*.rlib(extinfo.o)  : error LNK2019: unresolved external symbol __imp_SetFileSecurityW referenced in ExtractACL20
fatal error LNK1120: 13 unresolved externals
```

### Root cause

`unrar_sys/build.rs` (0.5.8, and master as of 2026-08) links these Windows libraries:

```rust
if cfg!(windows) {
    println!("cargo:rustc-flags=-lpowrprof");
    println!("cargo:rustc-link-lib=shell32");
    if cfg!(target_env = "gnu") {
        println!("cargo:rustc-link-lib=pthread");
    }
    ...
}
```

…but the vendored unrar C++ code also uses `advapi32` APIs, which are **never declared**:

| unrar source file | Function | advapi32 API used |
|---|---|---|
| `pathfn.cpp` | `GetRarDataPath()` | `RegCloseKey`, `RegOpenKeyExW`, `RegQueryValueExW` |
| `system.cpp` | `IsUserAdmin()` | `AllocateAndInitializeSid`, `CheckTokenMembership`, `FreeSid` |
| `system.cpp` | `Shutdown()` | `OpenProcessToken`, `AdjustTokenPrivileges`, `LookupPrivilegeValueW` |
| `crypt.cpp` | `GetRnd()` | `CryptAcquireContextW`, `CryptReleaseContext`, `CryptGenRandom` |
| `extinfo.cpp` | `ExtractACL20()` | `SetFileSecurityW` |

That the project's convention is "declare what you link" is visible all around this very
crate: `build.rs` declares `shell32` and `powrprof`, and the vendored sources use
`#pragma comment(lib, "wbemuuid.lib")` (`isnt.cpp:30`). `advapi32` is the one omission.

### Why this has been silent

1. **Until Rust 1.87.0 (2025-05-15)**: rustc linked `advapi32` by default as part of
   `std` on Windows MSVC, so the missing declaration was invisible. That default was
   removed in [rust-lang/rust#138233](https://github.com/rust-lang/rust/pull/138233).
2. **Since Rust 1.87.0**: the defect only surfaces as long as *no other crate* in the
   dependency tree declares `advapi32`. Any crate that does (e.g. `zpaq_rs`, which links
   `advapi32` for its own CryptoAPI usage) silently masks it. This is the same masking
   pattern previously reported in [#12](https://github.com/muja/unrar.rs/issues/12)
   (2018, `user32` symbols masked by an unrelated dependency).
3. **No upstream CI coverage**: `unarc-rs` — the main consumer of this crate — only runs
   its CI on `ubuntu-latest` ([run_tests.yml](https://github.com/mkrueger/unarc-rs/blob/master/.github/workflows/run_tests.yml)),
   so the defect never surfaced in its ecosystem.

### This is a recurring pattern, not a one-off

Windows link failures caused by undeclared system-library dependencies of the vendored
C++ code have now been reported four times:

| Issue | Year | Missing symbol family | Outcome |
|---|---|---|---|
| [#12](https://github.com/muja/unrar.rs/issues/12) | 2018 | `user32` (`CharToOemA`, `CharUpperW`, …) | masked by unrelated dependency; eventually fixed |
| [#37](https://github.com/muja/unrar.rs/issues/37) | 2023 | `comsupp.lib` (MSVC-private, windows-gnu) | workaround: enable only with MSVC compiler |
| [#52](https://github.com/muja/unrar.rs/issues/52) | 2024 | `RAR*` DLL symbols on 32-bit x86 | closed as "not worth it", contribution welcome |
| **this issue** | 2026 | `advapi32` (CryptoAPI / token / ACL / registry) | — |

Each fix patched one library and left the next one undeclared. A systematic audit of all
Windows API usage in `vendor/unrar` (and a Windows CI job) would break the cycle; the
one-line fix below at least resolves the current failure.

### Reproduction

Minimal `Cargo.toml` with only `unrar` (no other dependencies) on Windows MSVC + Rust >= 1.87:

```toml
[package]
name = "unrar-link-repro"
version = "0.1.0"
edition = "2024"

[dependencies]
unrar = "0.5.8"
```

```rust
fn main() {
    let archive = unrar::Archive::new("dummy.rar");
    for entry in archive.open_for_listing().unwrap() {
        println!("{}", entry.unwrap().filename.display());
    }
}
```

`cargo build` fails with the 13 LNK2019 errors above.

**Controlled experiment (the "masking" proof):** adding `zpaq_rs = "1.0"` to the same
`Cargo.toml` makes the link **succeed** — `zpaq_rs/build.rs` declares `advapi32`, and
cargo's `cargo:rustc-link-lib` has no scoping, so it applies to every final link target
in the dependency tree. Same machine, same `unrar_sys` version, same unrar call site:
one added dependency flips the result from fail to pass.

### Suggested fix

One line in `unrar_sys/build.rs`:

```rust
if cfg!(windows) {
    println!("cargo:rustc-flags=-lpowrprof");
    println!("cargo:rustc-link-lib=shell32");
    println!("cargo:rustc-link-lib=advapi32"); // ← add this
    ...
}
```

Recommendation: while touching this, audit the vendored sources for other Windows API
usage (`user32`, `ole32`, `gdi32`, …) so the system-library declarations are complete
instead of relying on rustc defaults or on other crates in the dependency tree.

### Related

- Precedent for the masking pattern: [#12](https://github.com/muja/unrar.rs/issues/12)
- rustc change that un-masked this: [rust-lang/rust#138233](https://github.com/rust-lang/rust/pull/138233)
- Official release notes (responsibility statement): [Rust 1.87.0](https://github.com/rust-lang/rust/releases/tag/1.87.0)

---
