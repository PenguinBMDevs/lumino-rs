use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, OnceLock},
};

pub mod menu;
pub mod window;

static EVENT_BUFFER: OnceLock<Mutex<EventBuffer>> = OnceLock::new(); // 事件缓冲区，用于存储事件
static EVENT_WAKER: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

pub fn set_waker(waker: impl Fn() + Send + Sync + 'static) {
    let _ = EVENT_WAKER.set(Box::new(waker));
}

#[derive(Debug, Clone)]
/// 事件
pub enum Event {
    Menu(menu::Event),     // 菜单事件
    Window(window::Event), // 窗口事件
}

impl Event {
    /// 获取事件的人类可读显示名称
    pub fn display_name(&self) -> String {
        match self {
            Self::Menu(e) => e.display_name(),
            Self::Window(e) => e.display_name(),
        }
    }

    // ── 构造函数（替代 event! 宏，IDE 友好） ──

    pub fn menu_file(event: menu::file::Event) -> Self {
        Self::Menu(menu::Event::File(event))
    }

    pub fn menu_edit(event: menu::edit::Event) -> Self {
        Self::Menu(menu::Event::Edit(event))
    }

    pub fn menu_view(event: menu::view::Event) -> Self {
        Self::Menu(menu::Event::View(event))
    }

    pub fn menu_help(event: menu::help::Event) -> Self {
        Self::Menu(menu::Event::Help(event))
    }

    pub fn window(event: window::Event) -> Self {
        Self::Window(event)
    }
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
        .unwrap_or_else(|e| {
            tracing::error!("Event mutex poisoned, recovering guard. This indicates a panic in event handling code.");
            e.into_inner()
        })
}

/// 推送事件到事件缓冲区
pub fn emit(event: Event) {
    buffer().push(event);
    if let Some(waker) = EVENT_WAKER.get() {
        waker();
    }
}

/// 从事件缓冲区中取出所有事件
pub fn take_events() -> Vec<Event> {
    buffer().take_all()
}
