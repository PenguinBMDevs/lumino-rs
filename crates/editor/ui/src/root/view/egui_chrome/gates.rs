//! 叠加层接管开关与「iced 弹层让位」判定。
//!
//! 从 `egui_chrome.rs` 拆出（REF-001：单文件 < 400 行）。

use crate::root::Root;

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

impl Root {
    /// 是否存在仍由 iced 绘制、且会被叠加层盖住的弹出层。
    ///
    /// 自绘标题栏的菜单、工具栏的精度/洋葱皮下拉与「更多」溢出菜单、悬浮绘制
    /// 工具条及其五个锚定面板已由 egui 接管；只有系统原生标题栏模式下 iced 仍
    /// 负责菜单下拉。
    pub(super) fn has_iced_popup(&self) -> bool {
        self.settings.synth.use_native_titlebar && self.state.is_menu_open
    }
}
