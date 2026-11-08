//! 叠加层接管开关。
//!
//! 从 `egui_chrome.rs` 拆出（REF-001：单文件 < 400 行）。

/// 钢琴卷帘骨架接管开关：默认关闭。
///
/// 生产里卷帘的键盘 / 标尺 / 网格由 `lumino-gfx` 的 wgpu 通道绘制（含主题化
/// 配色），iced canvas 的同名几何被它整块覆盖。egui 骨架在 gfx 退出该区域之前
/// 默认不接管，否则会把用户当前的网格配色盖成 iced 的深色键盘（视觉回归）。
/// 需要实验时置 `LUMINO_EGUI_PIANOROLL=1`。
pub(super) fn piano_roll_overlay_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("LUMINO_EGUI_PIANOROLL").is_some_and(|value| value != "0"))
}
