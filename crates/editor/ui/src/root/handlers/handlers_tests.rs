use super::*;
use crate::root::Root;
use lumino_core::storage::config::UiConfig;

// 拆分说明（2026-08-18）：原文件 657 行超 clippy `too-many-lines-threshold`（400），
// 按测试主题拆分为子模块：
// - `handler_behavior`：各消息处理器行为（协作对话框 / 自定义精度对话框 / 播放管理器 / 核心事件转发）
// - `piano_roll`：钢琴卷帘演奏指示线与上下文菜单
// - `velocity`：力度面板双向滚轮与 Tempo BPM 上限
// - `recover_track`：恢复已删除音轨对话框
// - `document_sync`：新建/恢复音轨后 document 同步与 PPQ 保存链路
mod document_sync;
mod handler_behavior;
mod piano_roll;
mod recover_track;
mod velocity;

fn create_root() -> Root {
    Root::new(&UiConfig::default())
}

/// 挂载测试 document 到 Root（当前轨 = 1，音符写入 document 单一权威源）
fn attach_test_document(root: &mut Root) {
    let doc = crate::test_helpers::make_test_document();
    root.set_midi_document(doc);
    root.editor.editor_state.data.current_track = 1;
}

#[test]
fn test_message_router_consumes_message() {
    struct ConsumingHandler;
    impl MessageHandler for ConsumingHandler {
        fn handle(&mut self, _root: &mut Root, _msg: Message) -> Option<Message> {
            None
        }
    }

    let mut router = MessageRouter::new();
    router.register(Box::new(ConsumingHandler));
    let mut root = create_root();

    // 不应 panic；消息被消费后不再继续传递
    router.route(&mut root, Message::ToggleSettings);
}

#[test]
fn test_message_router_falls_through_when_not_consumed() {
    use std::cell::RefCell;
    use std::rc::Rc;

    struct PassThroughHandler;
    impl MessageHandler for PassThroughHandler {
        fn handle(&mut self, _root: &mut Root, msg: Message) -> Option<Message> {
            Some(msg)
        }
    }

    let received: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    struct CapturingHandler {
        received: Rc<RefCell<bool>>,
    }
    impl MessageHandler for CapturingHandler {
        fn handle(&mut self, _root: &mut Root, _msg: Message) -> Option<Message> {
            *self.received.borrow_mut() = true;
            None
        }
    }

    let mut router = MessageRouter::new();
    let received2 = Rc::clone(&received);
    router.register(Box::new(PassThroughHandler));
    router.register(Box::new(CapturingHandler {
        received: received2,
    }));

    let mut root = create_root();
    router.route(&mut root, Message::ToggleSettings);

    // 由于第一个 handler 返回 Some，消息应落到第二个 handler
    assert!(*received.borrow(), "消息应透传到第二个处理器");
}

/// BUG 回归：无需重绘的 Sidebar 消息必须被判定为"已处理"。
///
/// 修复前 `try_handle_direct` 把 `handle_sidebar_event` 的"是否需要重绘"
/// 返回值当作"是否已处理"，导致 hover 移动（TrackReorderMoved）、重命名
/// 输入（TrackRenameChanged）等无状态变化事件误落入 MessageRouter，
/// 在 router 尾部刷"未处理的消息"噪音 WARN。
#[test]
fn test_sidebar_message_always_handled_directly() {
    let mut root = create_root();

    // 典型高频噪音源：音轨列表 hover 移动（未按下时 track_reorder 为 None，
    // 状态零变化 → 无需重绘）
    assert!(root.try_handle_direct(&Message::Sidebar(
        crate::sidebar::Event::TrackReorderMoved { x: 1.0, y: 1.0 },
    )));
    // 典型高频噪音源：重命名输入（只改 buffer，不触发重绘判定）
    assert!(root.try_handle_direct(&Message::Sidebar(
        crate::sidebar::Event::TrackRenameChanged(0, "New Name".into()),
    )));
    // 正常 Sidebar 事件同样必须已处理
    assert!(root.try_handle_direct(&Message::Sidebar(crate::sidebar::Event::TrackSelected(0),)));
}
