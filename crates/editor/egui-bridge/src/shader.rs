//! 与 `egui-wgpu 0.37` 语义一致的 WGSL 着色器。
//!
//! 差异仅两处：
//! - 顶点位置在着色器里乘 `pixels_per_point`（egui 顶点是逻辑点）；
//! - 过滤固定为确定性双线性 + ClampToEdge（本项目只上传字体图集与线性图标）。

/// egui 绘制着色器。
pub const EGUI_WGSL: &str = include_str!("egui.wgsl");
/// 顶点入口。
pub const VERTEX_ENTRY: &str = "vs_main";
/// gamma 目标（`Rgba8Unorm` 等）片元入口。
pub const FRAGMENT_GAMMA_ENTRY: &str = "fs_main_gamma";
/// sRGB 目标（`Rgba8UnormSrgb` 等）片元入口。
pub const FRAGMENT_SRGB_ENTRY: &str = "fs_main_srgb";
