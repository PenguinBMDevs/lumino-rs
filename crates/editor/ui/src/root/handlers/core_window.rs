//! 核心事件与窗口事件处理器
//!
//! 处理需要直接响应的消息：
//! - `Message::Core`：转发到事件总线
//! - `Message::Window`：同步 FPS / 性能数据并更新 Window 状态

use crate::event;
use crate::root::Root;
use crate::window;

impl Root {
    /// 处理核心事件
    pub(crate) fn handle_core_event(&mut self, event: event::Event) {
        self.set_menu_open(false);
        event::emit(event);
    }

    /// 处理窗口事件
    pub(crate) fn handle_window_event(&mut self, event: window::Event) {
        let is_fps_update = matches!(&event, window::Event::FpsUpdate(_));
        let is_theme_change = matches!(&event, window::Event::Theme(_));

        if is_fps_update && let window::Event::FpsUpdate(fps) = &event {
            self.statusbar.set_fps(*fps);
        }

        // PerfUpdate 通过 Message::Window(Event::PerfUpdate) 路由到此路径，
        // 直接转发到状态栏（否则被 window.update 吞没，数据显示全零）
        if let window::Event::PerfUpdate(data) = &event {
            self.statusbar.set_perf_data(*data);
        }

        self.window.update(event);

        if is_theme_change {
            self.editor.grid_cache.clear();
            self.editor.keyboard_cache.clear();
            self.editor.ruler_cache.clear();
        }
    }
}
