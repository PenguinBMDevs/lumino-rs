//! 生产状态 → egui 外壳的只读视图模型。
//!
//! `lumino-ui-egui` 不能依赖 `lumino-ui`（会引入 iced），因此由生产侧
//! （`Root::egui_chrome_model`）把状态填进这里的纯数据类型，egui 只负责绘制。
//! 这样同一份渲染代码既能被离线场景复用，也能被真实窗口叠加层复用。

mod batch_edit;
mod cloud_connect;
mod draw_settings;
mod help;
mod history;
mod memory_monitor;
mod right_panel;
mod settings_audio;
mod settings_cloud;
mod settings_page;
mod settings_ui;
mod tool_panel;
mod toolbar;
pub use batch_edit::*;
pub use cloud_connect::*;
pub use draw_settings::*;
pub use help::*;
pub use history::*;
pub use memory_monitor::*;
pub use right_panel::*;
pub use settings_audio::*;
pub use settings_cloud::*;
pub use settings_page::*;
pub use settings_ui::*;
pub use tool_panel::*;
pub use toolbar::*;

use super::piano_roll::PianoRollView;

/// 主窗口外壳的只读快照。
#[derive(Debug, Clone, Default)]
pub struct ChromeModel {
    /// 独立对话框整窗（设置 / 保存确认 …）；`Some` 时只画对话框，不画任何 band。
    pub dialog: Option<DialogModel>,
    /// 标题栏；`None` = 使用系统原生标题栏，egui 不接管。
    pub titlebar: Option<TitlebarModel>,
    /// 工具栏；`None` = 不接管。
    pub toolbar: Option<ToolbarModel>,
    /// 悬浮绘制工具条（含锚定面板）；`None` = 未打开或不接管。
    pub tool_panel: Option<ToolPanelModel>,
    /// 左侧栏；`None` = 不接管（拖拽 / 重命名 / 菜单等瞬态）。
    pub sidebar: Option<SidebarModel>,
    /// 右侧栏图标列；`None` = 不接管（当前视图不渲染右侧栏）。
    pub right_sidebar: Option<RightSidebarModel>,
    /// 钢琴卷帘骨架；`None` = 不接管（纵向卷帘 / 循环 / 拍号变化 / 洋葱皮 / 协作光标）。
    pub piano_roll: Option<PianoRollView>,
    /// 状态栏。
    pub statusbar: StatusbarModel,
}

mod chrome;
mod dialog;
mod sidebar;

pub use chrome::*;
pub use dialog::*;
pub use sidebar::*;
