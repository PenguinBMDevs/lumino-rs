//! 原生 egui 运行时（winit + wgpu 27 + egui，**无 旧版**）。
//!
//! 这是 `feat/egui-native-runtime` 分支的第一块地基：先用最小可运行窗口证明
//! 「winit 事件循环 + wgpu Surface + `lumino-egui-bridge` 绘制」这条链路在本项目
//! 固定的 wgpu 27 / egui 0.37 组合下可用，再逐步接入 `Root` / `Host`。
//!
//! 目前为探针程序：`cargo run -p lumino-ui-runtime --bin egui-runtime-probe`。

mod app;

pub use app::{NativeApp, run};
