use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, OnceLock},
};

pub mod cloud;
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
    Cloud(cloud::Event),   // 云存储事件
}

impl Event {
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

    pub fn cloud(event: cloud::Event) -> Self {
        Self::Cloud(event)
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // 事件缓冲区是全局单例，测试并行运行会互相污染，串行化测试
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_event_buffer_empty_on_start() {
        let _guard = TEST_MUTEX.lock().expect("测试串行锁未 poison");
        let events = take_events();
        assert!(events.is_empty());
    }

    #[test]
    fn test_emit_and_take_events() {
        let _guard = TEST_MUTEX.lock().expect("测试串行锁未 poison");
        // 先清空，避免其它测试残留事件干扰
        let _ = take_events();

        let e1 = Event::menu_file(menu::file::Event::New);
        let e2 = Event::menu_edit(menu::edit::Event::Undo);
        emit(e1.clone());
        emit(e2.clone());

        let events = take_events();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            Event::Menu(menu::Event::File(menu::file::Event::New))
        ));
        assert!(matches!(
            events[1],
            Event::Menu(menu::Event::Edit(menu::edit::Event::Undo))
        ));

        // 取出后缓冲区应为空
        let empty = take_events();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_event_constructors() {
        let _guard = TEST_MUTEX.lock().expect("测试串行锁未 poison");
        // Menu constructors
        let e = Event::menu_file(menu::file::Event::Open);
        assert!(matches!(
            e,
            Event::Menu(menu::Event::File(menu::file::Event::Open))
        ));

        let e = Event::menu_edit(menu::edit::Event::Copy);
        assert!(matches!(
            e,
            Event::Menu(menu::Event::Edit(menu::edit::Event::Copy))
        ));

        let e = Event::menu_view(menu::view::Event::ZoomIn);
        assert!(matches!(
            e,
            Event::Menu(menu::Event::View(menu::view::Event::ZoomIn))
        ));

        let e = Event::menu_help(menu::help::Event::About);
        assert!(matches!(
            e,
            Event::Menu(menu::Event::Help(menu::help::Event::About))
        ));

        // Window constructors
        let e = Event::window(window::Event::drag());
        assert!(matches!(
            e,
            Event::Window(window::Event::Lifecycle(window::lifecycle::Event::Drag))
        ));

        let e = Event::window(window::Event::close());
        assert!(matches!(
            e,
            Event::Window(window::Event::Lifecycle(window::lifecycle::Event::Close))
        ));
    }

    #[test]
    fn test_event_debug_and_clone() {
        let _guard = TEST_MUTEX.lock().expect("测试串行锁未 poison");
        let e = Event::menu_file(menu::file::Event::New);
        assert!(format!("{:?}", e).contains("New"));

        let cloned = e.clone();
        assert_eq!(format!("{:?}", e), format!("{:?}", cloned));
    }

    #[test]
    fn test_buffer_is_empty_after_take_all() {
        let _guard = TEST_MUTEX.lock().expect("测试串行锁未 poison");
        // 先清空，避免其它测试残留事件干扰
        let _ = take_events();

        emit(Event::menu_file(menu::file::Event::New));
        emit(Event::menu_file(menu::file::Event::Save));
        let events = take_events();
        assert_eq!(events.len(), 2);
        assert!(buffer().is_empty());
    }
}
