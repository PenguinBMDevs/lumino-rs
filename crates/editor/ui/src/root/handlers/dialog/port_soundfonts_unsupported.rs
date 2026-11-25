//! 「端口音色库未生效」提示窗处理器（REND-005）。

use crate::host::DialogResult;
use crate::message::{Message, PortSoundfontsUnsupportedAction};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_port_soundfonts_unsupported(
        &self,
        root: &mut Root,
        action: PortSoundfontsUnsupportedAction,
    ) -> Option<Message> {
        match action {
            PortSoundfontsUnsupportedAction::Acknowledge => {
                // 映射已保存；用户已知晓当前使用全局音色库（降级运行）。
                root.state.dialog_result = Some(DialogResult::Cancel);
            }
            PortSoundfontsUnsupportedAction::OpenSettings => {
                root.state.dialog_result = Some(DialogResult::Cancel);
                // 复用菜单「设置」入口：Runner 会打开设置对话框，用户可切到 XSynth。
                crate::event::emit(crate::event::Event::menu_file(
                    crate::event::menu::file::Event::settings(),
                ));
            }
        }
        None
    }
}
