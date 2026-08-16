//! 构建脚本：显式声明 Windows 目标所需的系统库
//!
//! 背景：`unrar_sys` 的 C++ 代码使用了 advapi32 的 API
//! （CryptAcquireContextW / CheckTokenMembership / SetFileSecurityW / Reg* 等），
//! 但其 build.rs 未声明该库（上游缺陷，见 muja/unrar.rs issue）。
//! Rust 1.87.0（2025-05）起 std 不再默认链接 advapi32（rust-lang/rust#138233），
//! 官方 Compatibility Notes 明确要求依赖此隐式假设的 C 库显式链接；
//! 此前该缺陷在本项目被 `zpaq_rs` 的全局链接声明意外掩盖，移除后暴露。
//! 在此兜底声明，保证链接稳定（根治在上游 unrar_sys）。

fn main() {
    #[cfg(windows)]
    println!("cargo:rustc-link-lib=dylib=advapi32");
}
