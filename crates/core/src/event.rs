use std::{collections::VecDeque, sync::{Mutex, MutexGuard, OnceLock}};

pub mod menu;
pub mod window;

static EVENT_BUFFER: OnceLock<Mutex<EventBuffer>> = OnceLock::new();

#[derive(Debug, Clone)]
pub enum Event {
    Menu(menu::Event),
    Window(window::Event),
}

#[macro_export]
macro_rules! event {
    /* Window start */
    (Window.$($rest:tt)+) => {
        $crate::event::Event::Window(
            $crate::event::window::Event::$($rest)+
        )
    };
    /* Window end */

    /* Menu File start */
    (Menu.File.$($rest:tt)+) => {
        $crate::event::Event::Menu(
            $crate::event::menu::Event::File(
                $crate::event::menu::file::Event::$($rest)+
            )
        )
    };
    /* Menu File end */

    /* Menu Edit start */
    (Menu.Edit.$($rest:tt)+) => {
        $crate::event::Event::Menu(
            $crate::event::menu::Event::Edit(
                $crate::event::menu::edit::Event::$($rest)+
            )
        )
    };
    /* Menu Edit end */

    /* Menu View start */
    (Menu.View.$($rest:tt)+) => {
        $crate::event::Event::Menu(
            $crate::event::menu::Event::View(
                $crate::event::menu::view::Event::$($rest)+
            )
        )
    };
    /* Menu view end */

    /* Menu Help start */
    (Menu.Help.$($rest:tt)+) => {
        $crate::event::Event::Menu(
            $crate::event::menu::Event::Help(
                $crate::event::menu::help::Event::$($rest)+
            )
        )
    };
    /* Menu Help end */
}

#[derive(Debug, Default)]
pub struct EventBuffer {
    queue: VecDeque<Event>
}

impl EventBuffer {
    fn push(&mut self, event: Event) {
        self.queue.push_back(event);
    }

    pub fn take_all(&mut self) -> Vec<Event> {
        self.queue.drain(..).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

fn buffer<'a>() -> MutexGuard<'a, EventBuffer> {
    EVENT_BUFFER.get_or_init(|| {
        Mutex::new(EventBuffer::default())
    })
        .lock()
        .expect("Lock core EventBuffer")
}

pub fn emit(event: Event) {
    buffer().push(event)
}

pub fn take_events() -> Vec<Event> {
    buffer().take_all()
}
