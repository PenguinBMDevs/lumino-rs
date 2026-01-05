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
    (Window.$variant:ident) => {
        $crate::event::Event::Window(
            $crate::event::window::Event::$variant
        )
    };
    /* Window end */

    /* Menu File start */
    (Menu.File.$variant:ident) => {
        $crate::event::Event::Menu(
            $crate::event::menu::Event::File(
                $crate::event::menu::file::Event::$variant
            )
        )
    };
    /* Menu File end */

    /* Menu Edit start */
    (Menu.Edit.$variant:ident) => {
        $crate::event::Event::Menu(
            $crate::event::menu::Event::Edit(
                $crate::event::menu::edit::Event::$variant
            )
        )
    };
    /* Menu Edit end */

    /* Menu View start */
    (Menu.View.$variant:ident) => {
        $crate::event::Event::Menu(
            $crate::event::menu::Event::View(
                $crate::event::menu::view::Event::$variant
            )
        )
    };
    /* Menu view end */

    /* Menu Help start */
    (Menu.Help.$variant:ident) => {
        $crate::event::Event::Menu(
            $crate::event::menu::Event::Help(
                $crate::event::menu::help::Event::$variant
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
