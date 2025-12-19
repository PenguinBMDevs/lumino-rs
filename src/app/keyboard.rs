use iced::Task;

use crate::app::Message;

pub use iced::keyboard::{Event, listen};

pub fn handle(event: Event) -> Task<Message> {
    /* TODO */
    println!("Keyboard {event:?}");
    match event {
        _ => (),
    }
    Task::none()
}
