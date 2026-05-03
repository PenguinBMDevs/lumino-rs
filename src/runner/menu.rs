//! Runner 菜单事件处理
//!
//! 该模块已拆分为以下子模块：
//! - `file`: 文件菜单处理（新建、打开、保存、导入等）
//! - `view`: 视图菜单处理（主题切换等）
//! - `collaboration`: 协作功能处理（连接、创建房间、加入房间等）
//! - `window`: 窗口事件处理（对话框、协作事件等）

use crate::runner::RunnerInner;

pub mod collaboration;
pub mod edit;
pub mod file;
pub mod view;
pub mod window;

impl RunnerInner {
    /// 处理核心事件
    pub(super) fn process_core_events(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let events = lumino_core::event::take_events();
        for event in events {
            self.handle_core_event(event_loop, event);
        }

        // 定时同步协作状态（每 50ms 检查一次）
        self.sync_collaboration_if_needed();
    }

    /// 根据需要同步协作状态（50ms 节流）
    fn sync_collaboration_if_needed(&mut self) {
        let is_connected = self.collab_state.collaboration_service.is_connected();
        tracing::debug!(
            "sync_collaboration_if_needed: is_connected={}",
            is_connected
        );

        if !is_connected {
            return;
        }

        let now = std::time::Instant::now();
        let should_sync = match self.collab_state.last_collab_sync {
            None => true,
            Some(last) => now.duration_since(last).as_millis() >= 50,
        };

        if should_sync {
            self.sync_collaboration_state();
            self.collab_state.last_collab_sync = Some(now);
        }
    }

    /// 处理单个核心事件
    fn handle_core_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        event: lumino_core::event::Event,
    ) {
        use lumino_core::event::Event;

        match event {
            Event::Menu(menu_event) => {
                self.handle_menu_event(event_loop, menu_event);
            }
            Event::Window(window_event) => {
                self.handle_window_event(window_event);
            }
        }
    }

    /// 处理菜单事件
    fn handle_menu_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        menu_event: lumino_core::event::menu::Event,
    ) {
        use lumino_core::event::menu::Event::*;

        match menu_event {
            File(file_event) => {
                self.handle_file_menu_event(event_loop, file_event);
            }
            Edit(edit_event) => {
                self.handle_edit_menu_event(edit_event);
            }
            View(view_event) => {
                self.handle_view_menu_event(view_event);
            }
            Help(_) => {
                // 处理帮助事件（占位）
            }
        }
    }
}
