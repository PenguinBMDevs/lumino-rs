//! 「端口音色库未生效」提示窗处理器测试（REND-005）。

use crate::host::DialogResult;
use crate::message::{Message, PortSoundfontsUnsupportedAction};
use crate::root::Root;
use crate::state::root_state::DialogType;
use crate::test_helpers::event_queue_lock;

fn dialog_root() -> Root {
    let mut root = Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        DialogType::PortSoundfontsUnsupported,
    );
    root.set_port_soundfonts_unsupported_dialog("LGS (GPU)".to_string(), 2);
    root
}

/// 知晓并继续：只关闭提示，不改映射。
#[test]
fn acknowledge_closes_dialog() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();
    let mut root = dialog_root();

    root.update(Message::PortSoundfontsUnsupported(
        PortSoundfontsUnsupportedAction::Acknowledge,
    ));

    assert!(matches!(
        root.state.dialog_result,
        Some(DialogResult::Cancel)
    ));
}

/// 打开合成器设置：关闭提示并发出菜单「设置」事件。
#[test]
fn open_settings_emits_settings_event_and_closes() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();
    let mut root = dialog_root();

    root.update(Message::PortSoundfontsUnsupported(
        PortSoundfontsUnsupportedAction::OpenSettings,
    ));

    assert!(matches!(
        root.state.dialog_result,
        Some(DialogResult::Cancel)
    ));
    let events = lumino_message::events::take_events();
    assert!(
        events.iter().any(|event| matches!(
            event,
            lumino_message::events::Event::Menu(lumino_message::events::menu::Event::File(
                lumino_message::events::menu::file::Event::Settings
            ))
        )),
        "应发出菜单「设置」事件，实际: {events:?}"
    );
}
