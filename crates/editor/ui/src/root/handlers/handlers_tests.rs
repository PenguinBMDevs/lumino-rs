use super::*;
use crate::root::Root;
use crate::toast::ToastLevel;
use lumino_core::storage::config::{OnionSkinBackend, UiConfig};

// 拆分说明（2026-08-18）：原文件 657 行超 clippy `too-many-lines-threshold`（400），
// 按测试主题拆分为子模块：
// - `handler_behavior`：各消息处理器行为（协作对话框 / 自定义精度对话框 / 播放管理器 / 核心事件转发）
// - `piano_roll`：钢琴卷帘演奏指示线与上下文菜单
// - `velocity`：力度面板双向滚轮与 Tempo BPM 上限
// - `recover_track`：恢复已删除音轨对话框
// - `document_sync`：新建/恢复音轨后 document 同步与 PPQ 保存链路
// - `deferred_remote_ops`：远端音符操作与本地编辑临界区串行化（防索引漂移）
// - `cpu_guards`：CPU 洋葱皮后端下各不兼容**入口**的拦截（含 GPU 侧回归）
// - `rendering_panel_exit`：**出口**守卫（关闭音频/视频导出面板后的路由落点）
// - `mode_animation_guard`：瀑布流切换**过渡态**竞态（动画进行中切 CPU）
mod cpu_guards;
mod deferred_remote_ops;
mod document_sync;
mod handler_behavior;
mod help;
mod mask_panel;
mod mode_animation_guard;
mod piano_roll;
mod recover_track;
mod rendering_panel_exit;
mod selection_drift;
mod track_order;
mod velocity;

fn create_root() -> Root {
    Root::new(&UiConfig::default())
}

/// 构造指定洋葱皮后端的 Root（GPU 为默认后端；CPU 模拟「早已切到 CPU 后端」的会话）。
///
/// 三个守卫测试模块共用：入口闸、出口闸、过渡态守卫都必须同时验证 CPU 拦截与 GPU 回归。
pub(super) fn root_with_backend(backend: OnionSkinBackend) -> Root {
    let mut root = Root::new(&UiConfig::default());
    root.settings.hires.backend = backend;
    root
}

/// 最近一条 Toast 的 (级别, 文案)；无 Toast 时返回 `None`。
pub(super) fn last_toast(root: &Root) -> Option<(ToastLevel, String)> {
    root.toast
        .toasts()
        .last()
        .map(|toast| (toast.level, toast.message.clone()))
}

/// 断言「最后一条 Toast 是 Warning 且文案为 `expected`」。
pub(super) fn assert_warning_toast(root: &Root, expected: &str) {
    let (level, message) = last_toast(root).expect("拦截必须伴随一条 Toast（静默拒绝不可接受）");
    assert_eq!(level, ToastLevel::Warning, "拦截提示必须是 Warning 级");
    assert_eq!(message, expected, "Toast 文案必须是入口专属文案");
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
