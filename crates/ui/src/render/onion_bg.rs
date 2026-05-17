//! 洋葱皮背景瓦片渲染管线
//!
//! 包含 WGSL 着色器源码和将来渲染管线的初始化函数。
//! 着色器文件 `onion_bg.wgsl` 通过 `include_str!` 在编译时嵌入。

/// 洋葱皮背景瓦片着色器源码（编译时嵌入）
pub const SHADER_SRC: &str = include_str!("onion_bg.wgsl");
