//! Host 消息处理子模块 — process_message 及其辅助方法

use crate::host::Host;
use crate::{message, sidebar, toolbar, window};

impl Host {
    /// 处理窗口/工具栏/侧边栏拖拽消息
    fn try_handle_window_resize_message(&mut self, message: &message::Message) -> Option<bool> {
        match message {
            message::Message::Window(window::Event::TrafficAction(action)) => {
                self.window_ctx.pending_window_action = Some(action.clone());
                Some(false) // 窗口动作不需要 UI 重建
            }
            message::Message::Window(window::Event::ToggleMaximize) => {
                self.window_ctx.pending_window_action = Some(window::TrafficAction::ToggleMaximize);
                Some(false)
            }
            message::Message::Window(window::Event::Close) => {
                self.window_ctx.pending_window_action = Some(window::TrafficAction::Close);
                Some(false)
            }
            message::Message::Window(window::Event::Drag) => {
                self.window_ctx.pending_drag = true;
                Some(false)
            }
            // 处理工具栏调整大小事件
            message::Message::Toolbar(toolbar::Event::ResizeDragStarted(_)) => {
                if let Some(pos) = self.window_ctx.cursor_position {
                    self.window_ctx.is_toolbar_resizing = true;
                    self.root.toolbar.start_resize(pos.y);
                }
                Some(true) // 工具栏大小改变需要 UI 重建
            }
            message::Message::Toolbar(toolbar::Event::ResizeDragEnded) => {
                self.window_ctx.is_toolbar_resizing = false;
                self.root.toolbar.end_resize();
                Some(true)
            }
            // 处理侧边栏调整大小事件
            message::Message::Sidebar(sidebar::Event::ResizeDragStarted(_)) => {
                if let Some(pos) = self.window_ctx.cursor_position {
                    self.root.sidebar.start_resize(pos.x);
                }
                Some(true) // 侧边栏大小改变需要 UI 重建
            }
            message::Message::Sidebar(sidebar::Event::ResizeDragEnded) => {
                self.root.sidebar.end_resize();
                Some(true)
            }
            // 处理右侧栏调整大小事件
            // 必须在 Host 层用当前光标位置初始化拖拽锚点（与左侧栏对称）：
            // 若落入 Root handler 则只有 is_resizing 标记、锚点保持初始值
            // 0.0/200.0，增量计算深度为负导致面板回撤并卡死在最小宽度。
            message::Message::RightSidebar(
                lumino_message::RightSidebarAction::ResizeDragStarted,
            ) => {
                if let Some(pos) = self.window_ctx.cursor_position {
                    self.root.right_sidebar.start_resize(pos.x);
                }
                Some(true) // 右侧栏大小改变需要 UI 重建
            }
            message::Message::RightSidebar(lumino_message::RightSidebarAction::ResizeDragEnded) => {
                self.root.right_sidebar.end_resize();
                Some(true)
            }
            _ => None,
        }
    }

    /// 处理主题变更消息
    fn handle_theme_message(&mut self, message: message::Message) -> bool {
        puffin::profile_scope!("process_message::theme");
        self.route_message(message);
        self.root.editor.keyboard_cache.clear();
        self.root.editor.ruler_cache.clear();
        self.render_ctx.render_cache.grid_viewport_hash = 0;
        self.render_ctx.render_cache.note_viewport_hash = 0;
        self.render_ctx.render_cache.note_render_viewport = None;
        self.root.editor.grid_cache.clear();
        true
    }

    /// 处理编排操作消息（Copy/Paste/Cut/DeleteSelection）
    fn handle_arrangement_message(&mut self, message: message::Message) -> bool {
        let track_idx = self.root.editor.current_track() as u16;
        {
            puffin::profile_scope!("process_message::arrangement_op");
            self.route_message(message);
        }
        self.mark_waterfall_dirty(track_idx);
        self.window_ctx.window.request_redraw();
        true
    }

    /// 处理单个消息，返回是否有状态变更
    pub(crate) fn process_message(&mut self, message: message::Message) -> bool {
        // 处理窗口/工具栏/侧边栏拖拽消息
        {
            puffin::profile_scope!("process_message::window_match");
            if let Some(result) = self.try_handle_window_resize_message(&message) {
                return result;
            }
        }

        // 主题变更：需要同时失效 wgpu 网格/音符缓存以刷新颜色
        if matches!(&message, message::Message::Window(window::Event::Theme(_))) {
            return self.handle_theme_message(message);
        }

        // 编辑器动作必须通过 Host::handle_action 处理，确保高精度贴图脏标记被正确设置
        if let message::Message::EditorAction(action) = message {
            {
                puffin::profile_scope!("process_message::editor_action");
                self.handle_action(action);
            }
            return true;
        }

        // 工程走带操作：先通过 route_message 处理数据修改，再标记高精度贴图脏
        if matches!(
            &message,
            message::Message::ArrangementCopy
                | message::Message::ArrangementPaste
                | message::Message::ArrangementCut
                | message::Message::ArrangementDeleteSelection
        ) {
            return self.handle_arrangement_message(message);
        }

        // 在 route_message 前，捕获面板右键菜单打开时的鼠标位置
        if matches!(
            &message,
            message::Message::Sidebar(sidebar::Event::PanelContextMenuOpened)
        ) && let Some(pos) = self.window_ctx.cursor_position
        {
            self.root.sidebar.set_panel_context_menu_pos(pos.x, pos.y);
        }

        // 其他消息交给 root 处理，假设可能有状态变更
        {
            puffin::profile_scope!("process_message::route_message");
            self.route_message(message);
        }
        true
    }

    /// 获取并清除待处理的窗口动作
    pub fn take_window_action(&mut self) -> Option<window::TrafficAction> {
        self.window_ctx.pending_window_action.take()
    }

    /// 获取并清除待处理的拖动标记
    pub fn take_drag(&mut self) -> bool {
        let drag = self.window_ctx.pending_drag;
        self.window_ctx.pending_drag = false;
        drag
    }
}
