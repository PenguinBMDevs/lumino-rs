use iced::Task;

use crate::app::Message;

pub use iced::keyboard::{Event, listen};

pub fn handle(event: Event) -> Task<Message> {
    use Event::*;
    match event {
        KeyPressed { key, modifiers, repeat, .. } => {
            tracing::trace!(?key, ?modifiers, ?repeat, "Key pressed");
        },
        KeyReleased { key, modifiers, .. } => {
            tracing::trace!(?key, ?modifiers, "Key released");
        },
        _ => (),
    }
    Task::none()
}
