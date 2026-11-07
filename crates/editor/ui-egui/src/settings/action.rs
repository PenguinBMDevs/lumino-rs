//! 设置对话框动作：egui → 生产 `Message` 的唯一出口。
//!
//! 设置窗口是独立 OS 窗口，它的 Host 同样是叠加层的宿主：每帧绘制后取走动作，
//! 由 `Root::settings_action_message` 翻译成 `Message::Settings(...)` /
//! `Message::SettingsDialog(...)` 再走 `process_message`。

use lumino_core::storage::config::{EraserBehavior, OnionSkinBackend, TrackAddBehavior};

use crate::chrome::{AudioField, UiField};

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
    /// 音频页下拉选择（下标按模型选项顺序）。
    AudioCombo {
        /// 字段。
        field: AudioField,
        /// 选项下标。
        index: usize,
    },
    /// 音频页滑杆变化。
    AudioSlider {
        /// 字段。
        field: AudioField,
        /// 新值。
        value: f32,
    },
    /// 音频页文本输入（原样回传，合法性由生产解析）。
    AudioText {
        /// 字段。
        field: AudioField,
        /// 新文本。
        value: String,
    },
    /// 音频页开关。
    AudioToggle {
        /// 字段。
        field: AudioField,
        /// 新状态。
        checked: bool,
    },
    /// 音频页按钮。
    AudioButton {
        /// 字段。
        field: AudioField,
    },
    /// 界面页下拉选择。
    UiCombo {
        /// 字段。
        field: UiField,
        /// 选项下标。
        index: usize,
    },
    /// 界面页文本输入。
    UiText {
        /// 字段。
        field: UiField,
        /// 新文本。
        value: String,
    },
    /// 界面页开关。
    UiToggle {
        /// 字段。
        field: UiField,
        /// 新状态。
        checked: bool,
    },
    /// 界面页滑杆。
    UiSlider {
        /// 字段。
        field: UiField,
        /// 新值。
        value: f32,
    },
    /// 界面页按钮。
    UiButton {
        /// 字段。
        field: UiField,
    },
    /// 主题卡片点击（规范主题名）。
    UiTheme(String),
    /// 「确认」（关闭设置窗口并应用）。
    Confirm,
}

/// 记录本帧要回传给生产的动作。
///
/// **先到先得**：一帧内只保留第一个动作，避免页面重排后新控件被同帧点击误命中
/// 而覆盖掉真正的动作（例如下拉选项触发的页面切换）。
pub fn push_action(ctx: &egui::Context, action: SettingsAction) {
    let pending = ctx
        .data(|d| d.get_temp::<Option<SettingsAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<SettingsAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<SettingsAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}
