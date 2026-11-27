//! 端口音色库对话框处理器（REND-005，独立 OS 窗口）。

use lumino_core::tr;

use crate::host::DialogResult;
use crate::message::{Message, PortSoundfontsAction};
use crate::root::Root;
use crate::root::file_dialog::PathDialogTask;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_port_soundfonts(
        &self,
        root: &mut Root,
        action: PortSoundfontsAction,
    ) -> Option<Message> {
        match action {
            PortSoundfontsAction::Browse(port) => {
                // 文件对话框在后台线程打开（防 UI 冻结），结果由
                // `Root::poll_path_dialogs` 写回对应端口行。
                root.spawn_path_dialog(PathDialogTask::PortSoundfonts(port), move || {
                    lumino_platform::dialog::FileDialog::new()
                        .set_title(tr!("选择端口音色库"))
                        .add_filter(tr!("音色库文件"), &["sf2", "sfz"])
                        .add_filter(tr!("SF2 文件"), &["sf2"])
                        .add_filter(tr!("SFZ 文件"), &["sfz"])
                        .add_filter(tr!("所有文件"), &["*"])
                        .pick_file()
                });
            }
            PortSoundfontsAction::Clear(port) => {
                root.state.port_soundfonts_dialog.clear(port);
            }
            PortSoundfontsAction::Confirm => {
                let map = root.state.port_soundfonts_dialog.to_map();
                root.state.dialog_result = Some(DialogResult::PortSoundfonts(Box::new(map)));
            }
            PortSoundfontsAction::Cancel => {
                root.state.dialog_result = Some(DialogResult::Cancel);
            }
        }
        None
    }
}
