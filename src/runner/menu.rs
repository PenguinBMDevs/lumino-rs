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
                // 如果是鼠标移动，实时同步协作状态
                if matches!(window_event, lumino_core::event::window::Event::Drag) {
                    self.sync_collaboration_state();
                }
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
