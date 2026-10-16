use super::onion_send::{OnionSendOutcome, send_with_timeout};
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

// ── REND-019 PR-A′：洋葱皮通道 `send_timeout` 语义回归 ─────────────────────
// 背景（2026-10-17 release 实测）：`try_send` 立即丢弃使多块全量会话（Begin +
// N×Chunk + Done，大工程 100+ 条）必然残缺、`Done` 常丢 → 渲染侧永远收不到
// 完整会话 → 音符/洋葱皮整层不渲染。规范 §1.6 允许的 `send_timeout` 方案：
// 满通道等待消费者（背压，会话必然完整），等待上限内未被消费则超时丢弃。

/// 有消费者时：满通道发送等待并在超时窗口内送达（背压语义，不得丢弃）
#[test]
fn test_send_with_timeout_waits_for_consumer_and_delivers() {
    let (tx, rx) = crossbeam_channel::bounded::<u32>(1);
    tx.send(1).expect("预填通道失败");

    let consumer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let first = rx.recv().expect("消费者应收到第 1 条");
        let second = rx.recv().expect("消费者应收到第 2 条");
        (first, second)
    });

    let started = std::time::Instant::now();
    let outcome = send_with_timeout(&tx, 2, std::time::Duration::from_secs(2));
    assert_eq!(
        outcome,
        OnionSendOutcome::Sent,
        "有消费者时必须送达而非丢弃"
    );
    let (first, second) = consumer.join().expect("消费者线程 panic");
    assert_eq!((first, second), (1, 2), "消息顺序必须保持");
    assert!(
        started.elapsed() >= std::time::Duration::from_millis(40),
        "满通道发送应等待消费者腾挪，而非立即返回"
    );
}

/// 无消费者且通道满：超时丢弃——等待有界（UI 线程不被永久阻塞）
#[test]
fn test_send_with_timeout_is_bounded_when_no_consumer() {
    let (tx, _rx) = crossbeam_channel::bounded::<u32>(1);
    tx.send(1).expect("预填通道失败");

    let started = std::time::Instant::now();
    let outcome = send_with_timeout(&tx, 2, std::time::Duration::from_millis(50));
    assert_eq!(outcome, OnionSendOutcome::Timeout, "满且无消费者应超时");
    let elapsed = started.elapsed();
    assert!(
        elapsed >= std::time::Duration::from_millis(45),
        "应在超时窗口内保持等待，实际 {elapsed:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "超时必须是有界等待（不得永久阻塞），实际 {elapsed:?}"
    );
}

/// 接收端已释放：立即 Disconnected（快速失败，不等待超时窗口）
#[test]
fn test_send_with_timeout_disconnected_fails_fast() {
    let (tx, rx) = crossbeam_channel::bounded::<u32>(1);
    drop(rx);

    let started = std::time::Instant::now();
    assert_eq!(
        send_with_timeout(&tx, 1, std::time::Duration::from_secs(2)),
        OnionSendOutcome::Disconnected
    );
    assert!(
        started.elapsed() < std::time::Duration::from_millis(500),
        "断开应快速失败，不占用完整超时窗口"
    );
}
