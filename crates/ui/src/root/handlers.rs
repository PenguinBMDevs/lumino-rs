//! Root 消息处理器
//!
//! 采用分治法策略，将消息处理逻辑拆分为专门的处理器：
//! - CollaborationHandler: 协作功能
//! - DialogHandler: 对话框管理
//! - ToolbarHandler: 工具栏事件（播放控制统一通过此路径）
//!
//! 通过 MessageRouter 按顺序分发消息。

use crate::message::{EditorAction, Message};
use crate::root::Root;
use crate::{sidebar, window};

// 重新导出子模块
pub mod collaboration;
pub mod dialog;
pub mod toolbar;

// 重新导出处理器类型
pub use collaboration::CollaborationHandler;
pub use dialog::DialogHandler;
pub use toolbar::ToolbarHandler;

/// 消息处理器 trait
///
/// 实现此 trait 的类型可以处理特定类型的消息。
/// 如果返回 None，表示消息已处理完毕；
/// 如果返回 Some(msg)，表示消息需要传递给下一个处理器。
pub trait MessageHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message>;
}

/// 消息路由器
///
/// 按顺序将消息传递给各个处理器，直到有处理器处理完毕。
pub struct MessageRouter {
    handlers: Vec<Box<dyn MessageHandler>>,
}

impl MessageRouter {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    pub fn register(&mut self, handler: Box<dyn MessageHandler>) {
        self.handlers.push(handler);
    }

    pub fn route(&mut self, root: &mut Root, msg: Message) {
        let mut current_msg = Some(msg);

        for handler in &mut self.handlers {
            if let Some(msg) = current_msg {
                current_msg = handler.handle(root, msg);
            } else {
                break;
            }
        }

        // 如果还有未处理的消息，记录警告
        if let Some(unhandled) = current_msg {
            tracing::warn!("未处理的消息: {:?}", std::mem::discriminant(&unhandled));
        }
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl Root {
    /// 创建配置好的消息路由器
    pub fn create_message_router() -> MessageRouter {
        let mut router = MessageRouter::new();
        router.register(Box::new(CollaborationHandler::new()));
        router.register(Box::new(DialogHandler::new()));
        router.register(Box::new(ToolbarHandler::new()));
        router
    }

    /// 主更新入口 - 简化为路由分发
    pub fn update(&mut self, msg: Message) {
        // 使用消息路由器处理
        let mut router = Self::create_message_router();
        let remaining = self.route_message(msg, &mut router);

        // 处理未被专门处理器处理的消息
        if let Some(msg) = remaining {
            self.handle_remaining_messages(msg);
        }
    }

    /// 路由消息到专门的处理器
    fn route_message(&mut self, msg: Message, router: &mut MessageRouter) -> Option<Message> {
        // 尝试直接处理消息
        if self.try_handle_direct(&msg) {
            return None;
        }

        // 其他消息通过路由器处理
        router.route(self, msg);
        None
    }

    /// 直接处理不需要路由的消息
    /// 返回 true 表示消息已被处理
    fn try_handle_direct(&mut self, msg: &Message) -> bool {
        match msg {
            Message::Core(event) => {
                self.handle_core_event(event.clone());
                true
            }
            Message::Window(event) => {
                self.handle_window_event(event.clone());
                true
            }
            Message::Sidebar(event) => {
                // 返回 handle_sidebar_event 的结果，让调用者知道是否需要重新渲染
                self.handle_sidebar_event(event.clone())
            }
            Message::EditorAction(action) => {
                self.handle_editor_action(action.clone());
                true
            }
            _ => self.try_handle_simple_state(msg),
        }
    }

    /// 处理简单的状态更新消息
    fn try_handle_simple_state(&mut self, msg: &Message) -> bool {
        match msg {
            Message::Progress(progress) => {
                self.progress = progress.clone();
                true
            }
            Message::ScrollbarScrolled(x) => {
                self.editor.set_scroll_x(*x);
                true
            }
            Message::ScrollbarScrolledY(y) => {
                self.editor.set_scroll_y(*y);
                true
            }
            Message::ZoomXChanged { zoom, fixed_ratio } => {
                self.editor.set_zoom_x(*zoom, *fixed_ratio);
                true
            }
            Message::ZoomYChanged { zoom, fixed_ratio } => {
                self.editor.set_zoom_y(*zoom, *fixed_ratio);
                true
            }
            Message::CanvasBoundsChanged { offset, size } => {
                self.editor.set_canvas_offset(*offset);
                self.editor
                    .set_canvas_size(iced_core::Point::new(size.width, size.height));
                true
            }
            Message::MenuStateChanged(is_open) => {
                self.state.is_menu_open = *is_open;
                true
            }
            Message::Settings(event) => {
                self.settings.update(event.clone());
                // 如果是橡皮擦行为变更，同步到编辑器
                if let crate::settings::Event::EraserBehaviorChanged(behavior) = event {
                    self.editor.set_eraser_behavior(*behavior);
                }
                // 如果是力度过滤阈值变更，同步到 Root
                if let crate::settings::Event::VelocityFilterThresholdChanged(value) = event {
                    if let Ok(val) = value.parse::<u8>() {
                        self.velocity_filter_threshold = val;
                        tracing::debug!("Root: 力度过滤阈值同步为 {}", val);
                    }
                }
                // 自动滚动配置变更，同步到编辑器
                if let crate::settings::Event::AutoScrollFixedPositionChanged(value) = &event {
                    if let Ok(val) = value.parse::<u32>() {
                        let mut config = self.editor.auto_scroll_config().clone();
                        config.fixed_indicator_position = val;
                        self.editor.set_auto_scroll_config(config);
                        tracing::debug!("Root: 自动滚动固定位置同步为 {}", val);
                    }
                }
                if let crate::settings::Event::AutoScrollPageTriggerOffsetChanged(value) = &event {
                    if let Ok(val) = value.parse::<u32>() {
                        let mut config = self.editor.auto_scroll_config().clone();
                        config.page_trigger_offset = val;
                        self.editor.set_auto_scroll_config(config);
                        tracing::debug!("Root: 自动滚动翻页触发偏移同步为 {}", val);
                    }
                }
                if let crate::settings::Event::AutoScrollPageReturnPositionChanged(value) = &event {
                    if let Ok(val) = value.parse::<u32>() {
                        let mut config = self.editor.auto_scroll_config().clone();
                        config.page_return_position = val;
                        self.editor.set_auto_scroll_config(config);
                        tracing::debug!("Root: 自动滚动翻页返回位置同步为 {}", val);
                    }
                }
                // HiDPI 图标渲染变更，同步到图标缓存
                if let crate::settings::Event::IconHiDPIChanged(enabled) = &event {
                    crate::resources::icon::set_hidpi_enabled(*enabled);
                    tracing::debug!("Root: HiDPI 图标渲染切换为 {}", enabled);
                }
                true
            }
            Message::ToggleSettings | Message::Null => true,
            _ => false,
        }
    }

    /// 处理剩余的消息（备用）
    fn handle_remaining_messages(&mut self, msg: Message) {
        // 未处理的消息（通常意味着需要扩展处理器）
        let _ = std::mem::discriminant(&msg);
    }

    // ====================================================================
    // 核心事件处理
    // ====================================================================

    fn handle_core_event(&mut self, event: lumino_core::event::Event) {
        self.set_menu_open(false);
        lumino_core::event::emit(event);
    }

    fn handle_window_event(&mut self, event: window::Event) {
        // macOS: 将 FPS 事件转发到状态栏（仅在调试模式下）
        let is_fps_update = cfg!(target_os = "macos")
            && cfg!(debug_assertions)
            && matches!(&event, window::Event::FpsUpdate(_));
        let is_theme_change = matches!(&event, window::Event::Theme(_));

        if is_fps_update && let window::Event::FpsUpdate(fps) = &event {
            self.statusbar.set_fps(*fps);
        }

        self.window.update(event);

        if is_theme_change {
            self.editor.grid_cache.clear();
            self.invalidate_onion_skin_cache();
        }
    }

    fn handle_sidebar_event(&mut self, event: sidebar::Event) -> bool {
        // 先检查是否是音轨切换
        let track_selected_idx = if let sidebar::Event::TrackSelected(idx) = &event {
            Some(*idx)
        } else {
            None
        };

        // 检查是否是洋葱皮开关
        let onion_skin_toggled = matches!(&event, sidebar::Event::TrackOnionSkinToggled(_));

        // 更新 sidebar，获取是否需要重新渲染
        let needs_redraw = self.sidebar.update(event);

        // 更新画布偏移
        let sidebar_width = self.sidebar.width() as f32;
        let current_offset = self.editor.canvas_offset;
        self.editor
            .set_canvas_offset(iced_core::Point::new(sidebar_width, current_offset.y));

        // 洋葱皮开关变化，使缓存失效
        if onion_skin_toggled {
            self.invalidate_onion_skin_cache();
        }

        // 如果是音轨切换，发送 Core 事件
        if let Some(track_idx) = track_selected_idx {
            tracing::debug!("Root: 发射音轨选择事件，音轨 {}", track_idx);
            lumino_core::event::emit(lumino_core::event::Event::Menu(
                lumino_core::event::menu::Event::File(
                    lumino_core::event::menu::file::Event::TrackSelected(track_idx),
                ),
            ));
        }

        needs_redraw
    }

    /// 处理编辑器动作
    pub(crate) fn handle_editor_action(&mut self, action: EditorAction) {
        let old_tick = self.editor.playback_position;
        self.editor.handle_action(action);
        let new_tick = self.editor.playback_position;

        // 检查播放位置是否变化
        if (old_tick - new_tick).abs() > f32::EPSILON
            && let Some(manager) = &mut self.playback_manager
        {
            manager.seek(new_tick);
        }

        // 检查音符数据是否变化
        if self.editor.notes_changed() {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
            // 音符变化影响洋葱皮缓存
            self.invalidate_onion_skin_cache();
        }
    }
}
