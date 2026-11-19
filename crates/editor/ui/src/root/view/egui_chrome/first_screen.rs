//! 第一屏（无面板 / 无工程）操作按钮 → 生产 `Message`。
//!
//! 与菜单 / 工具栏同一批事件构造器：新建与打开走未保存确认流程（`file::Event`），
//! 设置走文件菜单的同一个设置事件，协作复用工具栏的「打开协作对话框」事件。
//! 本模块只做翻译，不持有任何状态。

use lumino_ui_egui::chrome::first_screen::FirstScreenAction;

use crate::Message;
use crate::root::Root;

impl Root {
    /// 第一屏按钮动作 → 生产 `Message`。
    pub(crate) fn first_screen_action_message(&self, action: FirstScreenAction) -> Message {
        use crate::event::menu::file;

        match action {
            FirstScreenAction::NewProject => {
                Message::Core(crate::event::Event::menu_file(file::Event::new_file()))
            }
            FirstScreenAction::OpenProjectFile => {
                Message::Core(crate::event::Event::menu_file(file::Event::open()))
            }
            FirstScreenAction::OpenProjectFolder => Message::Core(crate::event::Event::menu_file(
                file::Event::open_project_folder(),
            )),
            FirstScreenAction::OpenSettings => {
                Message::Core(crate::event::Event::menu_file(file::Event::settings()))
            }
            FirstScreenAction::OpenCollaboration => {
                crate::toolbar::Event::open_collaboration_dialog()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lumino_ui_egui::chrome::first_screen::FirstScreenAction;

    use crate::root::Root;

    /// 每个按钮都必须翻译出一条事件：漏一个 = 按钮点了没反应。
    #[test]
    fn test_every_first_screen_action_maps_to_message() {
        let root = Root::new(&lumino_core::storage::config::UiConfig::default());
        for action in [
            FirstScreenAction::NewProject,
            FirstScreenAction::OpenProjectFile,
            FirstScreenAction::OpenProjectFolder,
            FirstScreenAction::OpenSettings,
            FirstScreenAction::OpenCollaboration,
        ] {
            // 只验证能构造出事件（`Message` 不实现 PartialEq，无法逐字比对）。
            let _ = root.first_screen_action_message(action);
        }
    }
}
