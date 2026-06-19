fn main() {
    let git_hash = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|r| String::from_utf8(r.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);

    // Windows 子系统 cfg 标记控制：
    //
    // 通过 DEBUG 环境变量区分 profile（Cargo 在 build script 中注入）：
    //   - debug profile:   DEBUG=true  → 不发射标记（默认 CONSOLE 子系统）
    //   - fast-release:    DEBUG=true  → 不发射标记（默认 CONSOLE 子系统）
    //   - release profile: DEBUG=false → 发射 windows_gui_subsystem 标记
    //     → main.rs 中的 #![cfg_attr] 据此激活 windows_subsystem = "windows"
    //
    // 为什么不用 PROFILE env var？Cargo 对自定义 profile 会返回继承源名
    // （如 fast-release → "release"），无法区分。DEBUG 要可靠得多。
    //
    // 为什么不用 linker flag？windows_subsystem 属性还负责入口点
    // （WinMain 包装），只传 linker flag 会 LNK2019 链接报错。
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        let debug_enabled = std::env::var("DEBUG").unwrap_or_default();
        if debug_enabled != "true" {
            // 正式发布（release profile）：隐藏控制台窗口
            println!("cargo:rustc-cfg=windows_gui_subsystem");
        }
    }
}
