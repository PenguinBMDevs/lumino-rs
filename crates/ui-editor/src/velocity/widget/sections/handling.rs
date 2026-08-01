//! 事件处理方法子模块
//!
//! 按编辑模式（Velocity/Tempo/Automation）将处理逻辑拆分为子模块。

pub(super) mod automation;
pub(super) mod events;
pub(super) mod hover;
pub(super) mod tempo;
pub(super) mod velocity;

use iced_widget::canvas;
use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

/// 发布 VelocityAction 消息
pub(super) fn publish_velocity(action: VelocityAction) -> canvas::Action<Message> {
    canvas::Action::publish(Message::Velocity(action))
}
