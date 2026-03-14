use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, OnceLock},
};

pub mod menu;
pub mod window;

static EVENT_BUFFER: OnceLock<Mutex<EventBuffer>> = OnceLock::new(); // 事件缓冲区，用于存储事件

#[derive(Debug, Clone)]
/// 事件
pub enum Event {
    Menu(menu::Event),     // 菜单事件
    Window(window::Event), // 窗口事件
}

#[macro_export]
/// 事件宏
macro_rules! event {
    /* Window start */
    (Window.$($rest:tt)+) => { // 窗口事件宏
        $crate::event::Event::Window( // 窗口事件
            $crate::event::window::Event::$($rest)+ // 窗口事件子项
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
/// 事件缓冲区
pub struct EventBuffer {
    queue: VecDeque<Event>,
}

/// 事件缓冲区实现
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

/// 获取事件缓冲区
///
/// 如果 mutex 被 poison（线程 panic），会尝试恢复并继续使用该锁
fn buffer<'a>() -> MutexGuard<'a, EventBuffer> {
    EVENT_BUFFER
        .get_or_init(|| Mutex::new(EventBuffer::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// 推送事件到事件缓冲区
pub fn emit(event: Event) {
    buffer().push(event)
}

/// 从事件缓冲区中取出所有事件
pub fn take_events() -> Vec<Event> {
    buffer().take_all()
}
