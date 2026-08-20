//! 侧边栏事件构造器与按钮提示文本单元测试

use super::{Event, RollBarButton};
use crate::Message;
use iced_core::Color;
use lumino_extras::i18n::Language;
use lumino_message::TrackContextMenuItem;

#[test]
fn test_track_context_menu_event_helpers() {
    let msg = Event::track_context_menu_opened(3);
    assert!(matches!(
        msg,
        Message::Sidebar(Event::TrackContextMenuOpened(3))
    ));

    let msg = Event::track_context_menu_closed();
    assert!(matches!(
        msg,
        Message::Sidebar(Event::TrackContextMenuClosed)
    ));

    let msg = Event::track_context_menu_item_clicked(2, TrackContextMenuItem::Delete);
    assert!(matches!(
        msg,
        Message::Sidebar(Event::TrackContextMenuItemClicked(
            2,
            TrackContextMenuItem::Delete
        ))
    ));
}

#[test]
fn test_track_rename_event_helpers() {
    let msg = Event::track_rename_started(1);
    assert!(matches!(
        msg,
        Message::Sidebar(Event::TrackRenameStarted(1))
    ));

    let msg = Event::track_rename_changed(1, "New Name".to_string());
    assert!(matches!(
        msg,
        Message::Sidebar(Event::TrackRenameChanged(1, _))
    ));

    let msg = Event::track_rename_confirmed(1);
    assert!(matches!(
        msg,
        Message::Sidebar(Event::TrackRenameConfirmed(1))
    ));
}

#[test]
fn test_track_color_event_helpers() {
    let color = Color::from_rgb(1.0, 0.0, 0.0);
    let msg = Event::track_color_selected(2, color);
    assert!(matches!(
        msg,
        Message::Sidebar(Event::TrackColorSelected(2, c)) if c == color
    ));
}

#[test]
fn test_roll_bar_event_helpers() {
    let msg = Event::roll_bar_toggled(RollBarButton::Horizontal);
    assert!(matches!(
        msg,
        Message::Sidebar(Event::RollBarToggled(RollBarButton::Horizontal))
    ));

    let msg = Event::roll_bar_toggled(RollBarButton::Vertical);
    assert!(matches!(
        msg,
        Message::Sidebar(Event::RollBarToggled(RollBarButton::Vertical))
    ));
}

#[test]
fn test_roll_bar_tooltip_switches_language() {
    assert_eq!(
        RollBarButton::Horizontal.tooltip(Language::ZhCn),
        "横向卷帘"
    );
    assert_eq!(
        RollBarButton::Vertical.tooltip(Language::EnUs),
        "Vertical Roll"
    );
}
