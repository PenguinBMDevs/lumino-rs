//! 构建脚本：显式声明 Windows 目标所需的系统库
//!
//! 背景：合并 `lumino-archive-reader` 后，`unarc-rs` 的 C++ 代码使用了 advapi32 的 API
//! （CryptAcquireContextW / CheckTokenMembership / SetFileSecurityW / Reg* 等），
//! 但其 build.rs 未声明该库（上游缺陷，见 muja/unrar.rs issue）。
//! Rust 1.87.0 起 std 不再默认链接 advapi32，官方 Compatibility Notes 明确要求依赖此隐式假设的
//! C 库显式链接；在此兜底声明，保证链接稳定（根治在上游 unarc_sys）。

fn main() {
    // 注意：build.rs 里的 `#[cfg(windows)]` 判断的是**宿主**平台（build script 本身
    // 就是为宿主编译的）。交叉编译到 Android 时宿主仍是 Windows，`#[cfg(windows)]`
    // 会为真，于是给 android 链接器塞了 `-ladvapi32` → `ld.lld: unable to find
    // library -ladvapi32`。必须读目标平台的 CARGO_CFG_TARGET_OS。
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=dylib=advapi32");
    }
}
