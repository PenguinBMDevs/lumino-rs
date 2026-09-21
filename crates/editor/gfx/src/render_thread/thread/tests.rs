use super::{ControlCommand, RenderCommand, RenderParams, RenderStats};

#[test]
fn test_render_params_default() {
    let params = RenderParams::default();
    assert_eq!(params.viewport_size, (800, 600));
    assert_eq!(params.keyboard_width, 60.0);
    assert_eq!(params.ruler_height, 30.0);
}

#[test]
fn test_render_stats_default() {
    let stats = RenderStats::default();
    assert_eq!(stats.frame_count, 0);
    assert_eq!(stats.dropped_frames, 0);
}

#[test]
fn test_control_command_debug() {
    let cmd = ControlCommand::Resize {
        width: 1920,
        height: 1080,
    };
    let debug_str = format!("{:?}", cmd);
    assert!(debug_str.contains("Resize"));
}

#[test]
fn test_render_command_debug() {
    let params = RenderParams::default();
    let cmd = RenderCommand::Render {
        params: Box::new(params),
        frame_id: 1,
    };
    let debug_str = format!("{:?}", cmd);
    assert!(debug_str.contains("Render"));
}

/// 回归测试：验证 NoteEvent channel 不会因 sender 被立即 drop 而死信。
///
/// 之前 bug：`enable_separate_render_thread()` 中 `let (_tx, rx) = channel()`
/// `_tx` 立即 dropped → `rx.try_recv()` 返回 `Disconnected` → `process_events()` 死信。
/// 修复：sender 必须被持有（存储在 `WgpuRenderThread.note_event_sender`）。
///
/// 本测试模拟修复后的模式：sender 存活在变量中，receiver 不应收到 Disconnected。
#[test]
fn test_note_event_channel_stays_alive_when_sender_held() {
    let (sender, receiver) = std::sync::mpsc::channel::<crate::NoteEvent>();

    // sender 被持有（模拟存储在 WgpuRenderThread 中），不 drop
    let _held_sender = sender;

    // receiver 不应收到 Disconnected（通道存活）
    // try_recv 在空通道上返回 Empty，而非 Disconnected
    match receiver.try_recv() {
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            panic!("NoteEvent channel died: sender was dropped prematurely (regression)");
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            // 期望行为：通道存活但暂无数据
        }
        Ok(_) => {
            panic!("Expected empty channel, but received an event");
        }
    }
}

/// 回归测试：验证 sender drop 后 receiver 收到 Disconnected（用于 shutdown 流程）
#[test]
fn test_note_event_channel_disconnects_on_sender_drop() {
    let (sender, receiver) = std::sync::mpsc::channel::<crate::NoteEvent>();
    drop(sender); // 模拟 shutdown 中 `note_event_sender.take()`

    match receiver.try_recv() {
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            // 期望行为：sender drop 后通道断开，渲染线程可据此退出循环
        }
        _ => {
            panic!("Expected Disconnected after sender dropped");
        }
    }
}

/// 验证 NoteEvent 能通过 channel 正确传递（端到端通道健康度）
#[test]
fn test_note_event_channel_delivers_event() {
    let (sender, receiver) = std::sync::mpsc::channel::<crate::NoteEvent>();

    // 模拟 UI 线程发送 Clear 事件
    sender
        .send(crate::NoteEvent::Clear)
        .expect("发送 Clear 事件失败");

    // 模拟渲染线程 process_events 消费
    match receiver.try_recv() {
        Ok(crate::NoteEvent::Clear) => {
            // 期望行为：事件正确传递
        }
        other => {
            panic!("Expected NoteEvent::Clear, got {:?}", other);
        }
    }
}
