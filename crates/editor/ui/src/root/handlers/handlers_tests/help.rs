//! 帮助系统消息与首次提示（DRAW-038）

use lumino_message::{HelpAction, Tool};

use super::create_root;

#[test]
fn test_help_query_and_selection_update_local_state() {
    let mut root = create_root();
    assert!(
        root.try_handle_direct(&crate::message::Message::Help(HelpAction::QueryChanged(
            "shortcut".into()
        )))
    );
    assert_eq!(root.state.help_dialog.query, "shortcut");
    assert_eq!(root.state.help_dialog.selected, 0);

    assert!(root.try_handle_direct(&crate::message::Message::Help(HelpAction::TopicSelected(2))));
    assert_eq!(root.state.help_dialog.selected, 2);
}

#[test]
fn test_first_tip_shows_once_and_can_reset() {
    let mut root = create_root();
    root.maybe_show_help_tip(Tool::Curve);
    assert_eq!(
        root.state.help_tips.active_topic().map(|topic| topic.id),
        Some("curve")
    );
    assert!(!root.state.help_tips.should_show("curve"));

    // 同一工具再次选中：不得覆盖/重复弹
    root.state.help_tips.dismiss();
    root.maybe_show_help_tip(Tool::Curve);
    assert!(root.state.help_tips.active_topic().is_none());

    root.reset_help_tips();
    assert!(root.state.help_tips.should_show("curve"));
}

#[test]
fn test_dismiss_tip_action() {
    let mut root = create_root();
    root.maybe_show_help_tip(Tool::Text);
    assert!(root.state.help_tips.active_topic().is_some());
    assert!(root.try_handle_direct(&crate::message::Message::Help(HelpAction::DismissTip)));
    assert!(root.state.help_tips.active_topic().is_none());
}
