//! 构建脚本：按 profile 决定是否编译「形状面板 → 形状抽屉」入口链路。
//!
//! 正式发布（`cargo build --release`）不编译悬浮条形状面板里的抽屉入口，
//! `fast-release` / `dev` / `test` 保留，供本地调试与测试使用。
//!
//! Cargo 不支持「按 profile 开关 feature」，且自定义 profile（`fast-release`）
//! 的 `PROFILE` 环境变量实测会归一化成 `release`，无法单独识别；因此判据取
//! 「`PROFILE` 不是 release，或 `DEBUG` 为 true」。仓库内只有 `fast-release`
//! 在 release 基础上显式 `debug = true`（见根 `Cargo.toml`），据此区分正式发布
//! 与快速调试构建。

use std::env;

fn main() {
    println!("cargo::rerun-if-env-changed=PROFILE");
    println!("cargo::rerun-if-env-changed=DEBUG");
    println!("cargo::rustc-check-cfg=cfg(lumino_shape_drawer_entry)");

    let profile = env::var("PROFILE").unwrap_or_default();
    let debug = env::var("DEBUG").unwrap_or_default();
    if profile != "release" || debug == "true" {
        println!("cargo::rustc-cfg=lumino_shape_drawer_entry");
    }
}
