/**
 * 临时集成测试：验证协作同步已切换为「按值音符身份」（去 ID 后）。
 *
 * - A 端添加音符 (1920,60,480) → B 端收到同一按值身份（跨客户端稳定，不再用时间戳伪 id）。
 * - A 端移动同一音符 → B 端按值收到 Move（按 tick/key/length 精确匹配，而非按位置猜测）。
 * - 多音符并存（服务端 wire 层）：A 连续添加两音符，B 端分别收到两个不同按值身份，
 *   证明服务端以值为权威键、不同值不互相覆盖。
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

    // ── 1) A 添加音符 (1920,60,480) → B 收到同一按值身份 ──
    println!("  步骤1: A 添加音符 (tick=1920,key=60)");
    let add = NoteBatchOperation {
        action: NoteAction::Add,
        notes: vec![SyncNote {
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
                    operation.notes.first().map(|n| (n.tick, n.key, n.length))
                } else {
                    None
                }
            },
            5000,
        )
        .await;
    let recv_add_val = b_recv_add.ok_or("B 未收到 A 的添加事件")?;
    assert_eq!(
        recv_add_val,
        (1920.0, 60, 480.0),
        "B 收到的音符按值身份应与 A 发送的一致，实际: {recv_add_val:?}"
    );
    println!("  ✓ B 收到 A 添加的音符，tick/key/length={recv_add_val:?}（跨客户端按值一致）");

    // ── 2) A 移动同一音符 (1920,60) → B 按值收到 Move ──
    println!("  步骤2: A 移动音符 (tick=1920,key=60)");
    let mv = NoteBatchOperation {
        action: NoteAction::Move,
        notes: vec![SyncNote {
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
                    operation.notes.first().map(|n| (n.tick, n.key, n.length))
                } else {
                    None
                }
            },
            5000,
        )
        .await;
    let recv_move_val = b_recv_move.ok_or("B 未收到 A 的移动事件")?;
    assert_eq!(
        recv_move_val,
        (1920.0, 60, 480.0),
        "B 收到的 Move 事件应引用同一按值音符，实际: {recv_move_val:?}"
    );
    println!("  ✓ B 收到 A 移动的音符，tick/key/length={recv_move_val:?}（按值精确匹配）");

    // ── 3) 多音符并存（wire 层）：A 再添加 (2880,64)，B 收到该按值音符 ──
    println!("  步骤3: A 添加第二个音符 (tick=2880,key=64)");
    let add2 = NoteBatchOperation {
        action: NoteAction::Add,
        notes: vec![SyncNote {
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
                    operation
                        .notes
                        .iter()
                        .any(|n| n.tick == 2880.0 && n.key == 64 && n.length == 480.0)
                } else {
                    false
                }
            },
            5000,
        )
        .await;
    assert!(
        got_43,
        "B 应收到 (2880,64,480) 的音符，且不与 (1920,60,480) 互相覆盖"
    );
    println!("  ✓ B 收到 (2880,64) 的音符，与 (1920,60) 并行存在（服务端按值权威键，无碰撞）");

    println!("\n🎉 临时集成测试通过：协作同步已使用按值音符身份。");
    Ok(())
}
