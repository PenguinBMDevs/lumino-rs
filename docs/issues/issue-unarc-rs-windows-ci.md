# Issue 3 · mkrueger/unarc-rs — no Windows CI coverage; RAR support broken for all Windows users since Rust 1.87

## Title

```
CI: only ubuntu-latest — RAR extraction fails to link on Windows for every downstream user since Rust 1.87 (LNK2019 advapi32, 13 symbols)
```

## Body

### Summary

`unarc-rs` has no Windows CI coverage (`run_tests.yml` runs only on `ubuntu-latest`),
which allowed a transitive link defect to ship to **every Windows user**: a minimal
project depending solely on `unarc-rs = "0.6"` fails to link on Windows MSVC with
Rust >= 1.87 — 13 unresolved `advapi32` symbols from `libunrar_sys`.

### Root cause (upstream, not this crate)

`unarc-rs` depends on `unrar = "0.5.8"` → `unrar_sys`, whose `build.rs` never declares
`advapi32` even though the vendored unrar C++ code uses it heavily
(`CryptAcquireContextW`, `CheckTokenMembership`, `SetFileSecurityW`, `Reg*`, …).
Rust 1.87.0 removed rustc's default `advapi32` linking
([rust-lang/rust#138233](https://github.com/rust-lang/rust/pull/138233)); the official
release notes state C libraries relying on that assumption "may need to explicitly link
advapi32". Upstream issue filed at
[muja/unrar.rs](https://github.com/muja/unrar.rs) (advapi32 missing from `unrar_sys/build.rs`).

**None of `unarc-rs`' dependency tree declares `advapi32`** (verified: `delharc`,
`sevenz-rust2`, `salzweg`, `bzip2-sys`, … all have no system-library declarations), so
there is nothing to mask the defect — **every `unarc-rs` Windows consumer is affected**.

### Why this crate never noticed

`run_tests.yml` has a single job on `ubuntu-latest`. The link defect is Windows-only, so
CI cannot catch it. (A Windows job would have caught this the moment Rust 1.87 shipped,
over a year ago.)

### Reproduction

```toml
[package]
name = "unarc-link-repro"
version = "0.1.0"
edition = "2024"

[dependencies]
unarc-rs = "0.6"
```

```rust
fn main() {
    let mut archive = unarc_rs::unified::ArchiveFormat::open_path("dummy.rar").unwrap();
    while let Ok(Some(entry)) = archive.next_entry() {
        let _ = entry.name();
    }
}
```

`cargo build` on Windows MSVC + Rust >= 1.87:

```
libunrar_sys-*.rlib(pathfn.o)   : error LNK2019: __imp_RegCloseKey / __imp_RegOpenKeyExW / __imp_RegQueryValueExW
libunrar_sys-*.rlib(system.o)   : error LNK2019: __imp_OpenProcessToken / __imp_AdjustTokenPrivileges / __imp_AllocateAndInitializeSid / __imp_CheckTokenMembership / __imp_FreeSid / __imp_LookupPrivilegeValueW
libunrar_sys-*.rlib(crypt.o)    : error LNK2019: __imp_CryptAcquireContextW / __imp_CryptReleaseContext / __imp_CryptGenRandom
libunrar_sys-*.rlib(extinfo.o)  : error LNK2019: __imp_SetFileSecurityW
fatal error LNK1120: 13 unresolved externals
```

Full repro workspace (scenario `unarc-only`) ships with the parallel upstream issue.

### Suggested actions

1. **Add a Windows job to CI** (`runs-on: windows-latest`, `cargo test` includes RAR
   cases) — this catches the current defect and prevents future Windows-only regressions
   (this crate already had a precedent: the 2019 `unrar_sys` `user32` incident was only
   found by a Windows user, not CI).
2. **Optional downstream hardening**: declare `advapi32` in a `build.rs` of `unarc-rs`
   (`#[cfg(windows)] println!("cargo:rustc-link-lib=advapi32");`), which fixes the link
   for all downstream users immediately without waiting for `unrar_sys`. This is the
   standard "final linker backstop" practice, not a substitute for the upstream fix.

### Related

- Upstream defect: `muja/unrar.rs` — `unrar_sys` missing `advapi32` declaration (issue filed in parallel)
- rustc change: [rust-lang/rust#138233](https://github.com/rust-lang/rust/pull/138233)
- Previous Windows-only link incident: [muja/unrar.rs#12](https://github.com/muja/unrar.rs/issues/12)

---
