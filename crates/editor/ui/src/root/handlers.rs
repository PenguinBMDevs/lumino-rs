//! Root 消息处理器
//!
//! 采用分治法策略，将消息处理逻辑拆分为专门的处理器：
//! - CollaborationHandler: 协作功能
//! - DialogHandler: 对话框管理
//! - ToolbarHandler: 工具栏事件（播放控制统一通过此路径）
//! - ArrangementHandler: 工程走带视图滚动/缩放/编辑
//! - Sidebar 事件由 Root::handle_sidebar_event 直接处理（不走 router）
//! - EditorActionHandler: 编辑器动作与钢琴卷帘上下文菜单
//! - StateUpdateHandler: 简单状态更新与动画帧
//! - CoreWindowHandler: 核心/窗口事件转发
//!
//! 通过 MessageRouter 按顺序分发消息。

use crate::message::Message;
use crate::root::Root;

// 重新导出子模块
pub mod arrangement;
pub mod cloud;
pub mod collaboration;
pub mod core_window;
pub mod dialog;
pub mod editor_action;
pub mod loop_range;
pub mod material;
pub mod settings;
pub mod sidebar;
pub mod state_update;
pub mod toolbar;
pub mod velocity;
pub mod video_clip_update;

// 重新导出处理器类型
pub use cloud::CloudHandler;
pub use collaboration::CollaborationHandler;
pub use dialog::DialogHandler;
pub use loop_range::LoopRangeHandler;
pub use settings::SettingsHandler;
pub use toolbar::ToolbarHandler;
pub use velocity::VelocityHandler;

/// 消息处理器 trait
///
/// 实现此 trait 的类型可以处理特定类型的消息。
/// 如果返回 None，表示消息已处理完毕；
/// 如果返回 Some(msg)，表示消息需要传递给下一个处理器。
pub trait MessageHandler {
    /// 处理一条消息，返回需要继续传递的消息（None 表示已处理完毕）
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message>;
}

/// 消息路由器
///
/// 按顺序将消息传递给各个处理器，直到有处理器处理完毕。
pub struct MessageRouter {
    handlers: Vec<Box<dyn MessageHandler>>,
}

impl MessageRouter {
    /// 创建一个空的消息路由器
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// 注册一个消息处理器
    pub fn register(&mut self, handler: Box<dyn MessageHandler>) {
        self.handlers.push(handler);
    }

    /// 将消息依次传递给所有已注册的处理器
    pub fn route(&mut self, root: &mut Root, msg: Message) {
        let mut current_msg = Some(msg);

        for handler in &mut self.handlers {
            if let Some(msg) = current_msg {
                current_msg = handler.handle(root, msg);
            } else {
                break;
            }
        }

        // 如果还有未处理的消息，记录警告。
        // 打印完整消息（历史版本只打印 Discriminant(n)，无法区分是哪个
        // 变体，噪音告警完全不可诊断）
        if let Some(unhandled) = current_msg {
            tracing::warn!("未处理的消息: {:?}", unhandled);
        }
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// 创建配置好的消息路由器
pub fn create_message_router() -> MessageRouter {
    let mut router = MessageRouter::new();
    router.register(Box::new(CollaborationHandler::new()));
    router.register(Box::new(CloudHandler::new()));
    router.register(Box::new(DialogHandler::new()));
    router.register(Box::new(SettingsHandler::new()));
    router.register(Box::new(VelocityHandler::new()));
    router.register(Box::new(LoopRangeHandler::new()));
    router.register(Box::new(ToolbarHandler::new()));
    router
}

impl Root {
    /// 直接处理不需要路由的消息
    /// 返回 true 表示消息已被处理
    pub(crate) fn try_handle_direct(&mut self, msg: &Message) -> bool {
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
                // 已处理：Sidebar 事件在 handle_sidebar_event 中执行完毕。
                // 注意不能把"是否需要重绘"当作"是否已处理"——否则无需重绘的
                // Sidebar 消息（如音轨列表 hover 移动、重命名输入）会误落入
                // MessageRouter 并在尾部刷"未处理的消息"噪音 WARN。
                self.handle_sidebar_event(event.clone());
                true
            }
            Message::EditorAction(action) => {
                self.handle_editor_action(action.clone());
                true
            }
            Message::PianoRollContextMenu(action) => {
                self.handle_piano_roll_context_menu(action.clone());
                true
            }
            Message::RightSidebar(action) => {
                self.handle_right_sidebar_action(action.clone());
                true
            }
            _ => self.try_handle_simple_state(msg),
        }
    }

    /// 处理单条消息（供测试与内部调用复用）
    ///
    /// 复刻应用运行时 `Host::route_message` 的分发组合：先尝试 `Root` 直接处理，
    /// 未处理的消息再交给 `MessageRouter` 路由到各专用处理器（Toolbar/Dialog/...）。
    /// 与运行时使用同一套分发逻辑，保证测试与产品行为一致。
    pub fn update(&mut self, message: Message) {
        // 批量消息先展开，使每个子消息都能走完整的 PPQ / 路由逻辑
        if let Message::Batch(messages) = message {
            for msg in messages {
                self.update(msg);
            }
            return;
        }

        // PPQ 编辑时：任何非 PPQ 编辑消息触发时自动确认（实现"点击任意 UI 位置保存"）
        if self.toolbar.ppq_editing {
            let is_ppq_edit_msg = matches!(
                &message,
                Message::Toolbar(crate::toolbar::Event::PpqEditChanged(_))
                    | Message::Toolbar(crate::toolbar::Event::PpqEditToggled(_))
                    | Message::Toolbar(crate::toolbar::Event::PpqEditConfirmed)
            );
            if !is_ppq_edit_msg {
                if let Ok(ppq) = self.toolbar.ppq_edit_buffer.parse::<u16>()
                    && (24..=32767).contains(&ppq)
                {
                    self.set_ppq(ppq);
                    tracing::info!("PPQ 已更新为 {}", ppq);
                }
                self.toolbar.ppq_editing = false;
                self.toolbar.ppq_edit_buffer.clear();
            }
        }

        if !self.try_handle_direct(&message) {
            let mut router = create_message_router();
            router.route(self, message);
        }
    }
}

#[cfg(test)]
mod handlers_tests;
