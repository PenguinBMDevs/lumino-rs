//! 设置对话框动作：egui → 生产 `Message` 的唯一出口。
//!
//! 设置窗口是独立 OS 窗口，它的 Host 同样是叠加层的宿主：每帧绘制后取走动作，
//! 由 `Root::settings_action_message` 翻译成 `Message::Settings(...)` /
//! `Message::SettingsDialog(...)` 再走 `process_message`。

use lumino_core::storage::config::{EraserBehavior, OnionSkinBackend, TrackAddBehavior};

const ACTION_ID: &str = "lumino.settings.action";

/// 设置对话框产生的动作（不依赖 iced）。
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsAction {
    /// 点击左侧菜单（索引 = 菜单顺序）。
    MenuSelected(usize),
    /// 橡皮擦行为切换。
    EraserBehavior(EraserBehavior),
    /// 添加音轨行为切换。
    TrackAddBehavior(TrackAddBehavior),
    /// 日志保留份数输入（原样回传，合法性由生产解析）。
    LogRetention(String),
    /// 洋葱皮渲染后端切换。
    OnionSkinBackend(OnionSkinBackend),
    /// 启动时执行 GPU 兼容性检查。
    GpuCheckOnStartup(bool),
    /// 启动时显示 GPU 警告（`false` = 不再显示）。
    GpuWarningSuppressed(bool),
    /// Domino 剪贴板互通。
    DominoClipboard(bool),
    /// 手动执行 GPU 兼容性检查。
    RunGpuCheck,
    /// 复制 GPU 诊断信息。
    CopyGpuDiagnostics,
    /// 「确认」（关闭设置窗口并应用）。
    Confirm,
}

/// 记录本帧要回传给生产的动作。
pub fn push_action(ctx: &egui::Context, action: SettingsAction) {
    // `remove_temp` 要求 `Default`：用 `Option` 承载，取用时展平。
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<SettingsAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<SettingsAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}
