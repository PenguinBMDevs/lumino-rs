//! iced 视觉基线工具链（UI parity harness）。
//!
//! 本 crate **不改变生产 UI 行为**，只负责：
//! - 固定窗口尺寸、主题、语言与 DPI，渲染 4 个代表性场景；
//! - 输出可重复比对的 PNG 基线（`tests/ui-baseline/iced/`）；
//! - 提供 `ui-diff` 图像对比工具，供后续 egui 草稿与重构验收使用。
//!
//! 设计说明见 `docs/2026-11-06-ui-parity-harness.md`。

/// 无头截图引擎（基于 `iced_wgpu` + `iced_runtime`，支持任意 DPI）。
pub mod capture;
/// 图像对比核心（逐像素统计 + 红/蓝差异热图）。
pub mod diff;
/// 固定的 iced 视觉基线场景清单。
pub mod scene;

pub use capture::CapturedImage;
pub use diff::{CompareOptions, CompareReport, CompareResult, compare_images, compare_rgba};
pub use scene::{DPI_SCALES, Scene, SceneId, baseline_config};

use std::path::PathBuf;

/// 仓库内默认基线目录：`tests/ui-baseline/iced/`。
///
/// 通过 `CARGO_MANIFEST_DIR` 回溯到 workspace 根目录，保证无论从哪个工作目录
/// 启动 `capture_iced`，输出位置都稳定。
pub fn baseline_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .ancestors()
        .nth(3)
        .map(PathBuf::from)
        .unwrap_or(manifest_dir);
    workspace_root
        .join("tests")
        .join("ui-baseline")
        .join("iced")
}
