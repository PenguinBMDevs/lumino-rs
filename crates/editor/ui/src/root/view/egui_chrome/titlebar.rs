//! 标题栏动作 → 生产 `window::Event`。
//!
//! 主窗口与对话框 popup 标题栏共用：`lumino-ui-egui` 只回传
//! [`TitlebarAction`]，这里翻译成既有的窗口消息（拖动 / 最大化 / 红绿灯）。

use lumino_ui_egui::chrome::TitlebarAction;

use crate::Message;
use crate::root::Root;
use crate::window;

impl Root {
    /// 标题栏动作 → 生产消息。
    pub(crate) fn titlebar_action_message(&self, action: TitlebarAction) -> Message {
        match action {
            TitlebarAction::Drag => window::Event::drag(),
            TitlebarAction::ToggleMaximize => window::Event::toggle_maximize(),
            TitlebarAction::Minimize => {
                window::Event::traffic_action(&window::TrafficAction::Minimize)
            }
            TitlebarAction::Close => window::Event::traffic_action(&window::TrafficAction::Close),
        }
    }
}
