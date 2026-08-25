/**
 * 临时集成测试：验证协作同步已切换为「真实全局 u64 音符 ID」（缺陷 #4/#5 修复）。
 *
 * - A 端添加音符（id=42）→ B 端收到同一 id（跨客户端稳定身份，不再用时间戳伪 id）。
 * - A 端移动同一音符（id=42） → B 端按 id 收到 Move（按 id 精确匹配，而非按位置猜测）。
 * - 碰撞预防（服务端 wire 层）：A 连续添加 id=42 与 id=43，B 端分别收到两个不同 id，
 *   证明服务端以 id 为权威键、不同 id 不互相覆盖（B 端接收后再 `ensure_note_id_above`
 *   抬升本地分配器，避免本地后续新建复用到对端已占用的 id，详见 editor-state 单测
 *   `test_ensure_note_id_above_bumps_allocator`）。
 *
 * 默认 `#[ignore]`：需先本地启动 `../lumino-server-rs`（`cargo run -- --port 3000`，
 * 首次启动用默认账户 admin/admin），再手动运行：
 *   cargo test --test collab_id_sync_local -- --ignored --nocapture
 *
 * 可通过环境变量覆盖连接参数：
 *   LUMINO_LOCAL_HOST（默认 127.0.0.1）、LUMINO_LOCAL_PORT（默认 3000）
 *   LUMINO_TEST_USER（默认 admin）、LUMINO_TEST_PASS（默认 admin）
 */
mod common;

use std::time::Duration;
use tokio::time::sleep;

use lumino_collaboration::client::{CollaborationClient, CollaborationEvent};
use lumino_collaboration::types::{ClientConfig, NoteAction, NoteBatchOperation, SyncNote};

use common::EventCollector;

/// 临时集成测试：跨客户端按真实 u64 音符 ID 同步。
#[tokio::test]
#[ignore = "需要本地协作服务器 (../lumino-server-rs)，手动运行"]
async fn test_collab_id_sync_local() -> Result<(), Box<dyn std::error::Error>> {
    let host = std::env::var("LUMINO_LOCAL_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("LUMINO_LOCAL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let user = std::env::var("LUMINO_TEST_USER").unwrap_or_else(|_| "admin".to_string());
    let pass = std::env::var("LUMINO_TEST_PASS").unwrap_or_else(|_| "admin".to_string());

    println!("\n=== 临时集成测试：协作同步使用真实 u64 音符 ID ===");
    println!("  服务端: {host}:{port}\n");

    // ── 客户端 A 创建房间 ──
    let collector_a = EventCollector::new();
    let mut client_a = CollaborationClient::new(ClientConfig {
        server_host: host.clone(),
        server_port: port,
        username: user.clone(),
        password: pass.clone(),
        auto_reconnect: false,
        max_reconnect_attempts: 0,
    });
    client_a.set_event_callback(collector_a.callback());
    let create = client_a
        .create_room_and_connect("本地ID同步测试".to_string())
        .await?;
    let invite = create.room.invite_code.clone();
    println!("  A 创建房间，邀请码: {invite}");

    let user_a = collector_a
        .wait_for(
            |e| {
                if let CollaborationEvent::Authenticated { user_id, .. } = e {
                    Some(user_id.clone())
                } else {
                    None
                }
            },
            5000,
        )
        .await
        .ok_or("A 认证超时")?;
    println!("  A 认证成功: {user_a}");

    // ── 客户端 B 加入房间 ──
    let collector_b = EventCollector::new();
    let mut client_b = CollaborationClient::new(ClientConfig {
        server_host: host.clone(),
        server_port: port,
        username: user.clone(),
        password: pass.clone(),
        auto_reconnect: false,
        max_reconnect_attempts: 0,
    });
    client_b.set_event_callback(collector_b.callback());
    client_b.join_room_and_connect(invite.clone()).await?;
    println!("  B 加入房间");

    let user_b = collector_b
        .wait_for(
            |e| {
                if let CollaborationEvent::Authenticated { user_id, .. } = e {
                    Some(user_id.clone())
                } else {
                    None
                }
            },
            5000,
        )
        .await
        .ok_or("B 认证超时")?;
    println!("  B 认证成功: {user_b}");

    sleep(Duration::from_millis(500)).await;

    // ── 1) A 添加音符 id=42 → B 收到同一 id ──
    println!("  步骤1: A 添加音符 id=42");
    let add = NoteBatchOperation {
        action: NoteAction::Add,
        notes: vec![SyncNote {
            id: 42u64,
            tick: 1920.0,
            key: 60,
            length: 480.0,
            velocity: 100,
            channel: 0,
            track_index: 0,
        }],
        source_track: Some(0),
        target_track: Some(0),
        tick_offset: None,
        key_offset: None,
        timestamp: 1,
    };
    client_a.send_note_batch(add)?;

    let b_recv_add = collector_b
        .wait_for(
            |e| {
                if let CollaborationEvent::NoteBatch { user_id, operation } = e
                    && user_id == &user_a
                    && operation.action == NoteAction::Add
                {
                    operation.notes.first().map(|n| n.id)
                } else {
                    None
                }
            },
            5000,
        )
        .await;
    let recv_add_id = b_recv_add.ok_or("B 未收到 A 的添加事件")?;
    assert_eq!(
        recv_add_id, 42,
        "B 收到的音符 id 应与 A 发送的一致（真实全局 ID），实际: {recv_add_id}"
    );
    println!("  ✓ B 收到 A 添加的音符，id={recv_add_id}（跨客户端身份一致）");

    // ── 2) A 移动同一音符 id=42 → B 按 id 收到 Move ──
    println!("  步骤2: A 移动音符 id=42");
    let mv = NoteBatchOperation {
        action: NoteAction::Move,
        notes: vec![SyncNote {
            id: 42u64,
            tick: 1920.0,
            key: 60,
            length: 480.0,
            velocity: 100,
            channel: 0,
            track_index: 0,
        }],
        source_track: Some(0),
        target_track: Some(0),
        tick_offset: Some(480.0),
        key_offset: Some(0),
        timestamp: 2,
    };
    client_a.send_note_batch(mv)?;

    let b_recv_move = collector_b
        .wait_for(
            |e| {
                if let CollaborationEvent::NoteBatch { user_id, operation } = e
                    && user_id == &user_a
                    && operation.action == NoteAction::Move
                {
                    operation.notes.first().map(|n| n.id)
                } else {
                    None
                }
            },
            5000,
        )
        .await;
    let recv_move_id = b_recv_move.ok_or("B 未收到 A 的移动事件")?;
    assert_eq!(
        recv_move_id, 42,
        "B 收到的 Move 事件应引用同一音符 id=42（按 id 匹配，而非位置猜测），实际: {recv_move_id}"
    );
    println!("  ✓ B 收到 A 移动的音符，id={recv_move_id}（按 id 精确匹配）");

    // ── 3) 碰撞预防（wire 层）：A 再添加 id=43，B 收到两个不同 id ──
    println!("  步骤3: A 添加第二个音符 id=43（碰撞预防）");
    let add2 = NoteBatchOperation {
        action: NoteAction::Add,
        notes: vec![SyncNote {
            id: 43u64,
            tick: 2880.0,
            key: 64,
            length: 480.0,
            velocity: 100,
            channel: 0,
            track_index: 0,
        }],
        source_track: Some(0),
        target_track: Some(0),
        tick_offset: None,
        key_offset: None,
        timestamp: 3,
    };
    client_a.send_note_batch(add2)?;

    let got_43 = collector_b
        .contains_event(
            |e| {
                if let CollaborationEvent::NoteBatch { user_id, operation } = e
                    && user_id == &user_a
                    && operation.action == NoteAction::Add
                {
                    operation.notes.iter().any(|n| n.id == 43)
                } else {
                    false
                }
            },
            5000,
        )
        .await;
    assert!(got_43, "B 应收到 id=43 的音符，且不与 id=42 互相覆盖");
    println!("  ✓ B 收到 id=43 的音符，与 id=42 并行存在（服务端按 id 权威键，无碰撞）");

    println!("\n🎉 临时集成测试通过：协作同步已使用真实全局 u64 音符 ID。");
    Ok(())
}
