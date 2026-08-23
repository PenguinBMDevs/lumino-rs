use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

use lumino_collaboration::{
    client::{CollaborationClient, CollaborationEvent},
    types::{ClientConfig, NoteAction, NoteBatchOperation, SyncNote},
};

// 工具函数：创建客户端并等待连接和认证
async fn setup_client(
    username: &str,
) -> (
    CollaborationClient,
    mpsc::UnboundedReceiver<CollaborationEvent>,
) {
    let config = ClientConfig {
        server_host: "localhost".to_string(),
        server_port: 3000,
        username: username.to_string(),
        password: String::new(),
        auto_reconnect: false,
        max_reconnect_attempts: 1,
    };

    let mut client = CollaborationClient::new(config);
    let (tx, rx) = mpsc::unbounded_channel();

    client.set_event_callback(move |event| {
        let _ = tx.send(event);
    });

    // 注意：连接操作已在 create_room_and_connect / join_room_and_connect 中完成，
    // 此处仅创建客户端并配置回调，无需单独调用 connect()。
    // 旧版 connect() 已随重构移除（见 #dead-code-cleanup）。

    (client, rx)
}

async fn run_sync_test(rate: u64, total_notes: u32) {
    println!("Setting up clients...");
    let (mut client_a, mut rx_a) = setup_client("TestA").await;
    let (mut client_b, mut rx_b) = setup_client("TestB").await;

    println!("A creating room and connecting...");
    // A: 创建房间并连接（新 API，同时完成 HTTP 创建 + WebSocket 连接 + 认证）
    let create_resp = client_a
        .create_room_and_connect("TestRoom".to_string())
        .await
        .expect("创建房间失败");

    let room_invite_code = create_resp.room.invite_code.clone();
    println!("Room created, invite_code={}", room_invite_code);

    // 等待 A 的 RoomCreated 事件
    let mut room_created = false;
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(100), rx_a.recv()).await
            && let CollaborationEvent::RoomCreated { room } = event
        {
            println!("A room created: {:?}", room);
            room_created = true;
            break;
        }
    }
    assert!(room_created, "A should receive RoomCreated event");

    // B: 加入房间并连接（新 API）
    client_b
        .join_room_and_connect(room_invite_code.clone())
        .await
        .unwrap_or_else(|e| panic!("加入房间失败: {e}"));

    // 等待 B 的 RoomJoined 事件
    let mut joined = false;
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(100), rx_b.recv()).await
        {
            println!("B event: {:?}", event);
            if let CollaborationEvent::RoomJoined { .. } = event {
                joined = true;
                break;
            }
        }
    }
    assert!(joined, "B should join room");

    println!("Starting sync test: {} notes/sec", rate);

    let delay = Duration::from_micros(1_000_000 / rate);

    // Test A -> B
    for i in 0..total_notes {
        let op = NoteBatchOperation {
            action: NoteAction::Add,
            notes: vec![SyncNote {
                id: (rate as u64) * 100000 + i as u64,
                tick: 0.0,
                key: 60,
                length: 480.0,
                velocity: 100,
                channel: 0,
                track_index: 0,
            }],
            source_track: None,
            target_track: None,
            tick_offset: None,
            key_offset: None,
            timestamp: i as u64,
        };
        client_a.send_note_batch(op).expect("发送音符批次失败");
        sleep(delay).await;
    }

    // Check if B received all
    let mut b_received = 0;
    let timeout = Duration::from_secs(3);
    let start = std::time::Instant::now();

    while b_received < total_notes && start.elapsed() < timeout {
        if let Ok(event) = tokio::time::timeout(Duration::from_millis(100), rx_b.recv()).await
            && let Some(CollaborationEvent::NoteBatch { .. }) = event
        {
            b_received += 1;
        }
    }

    assert_eq!(
        b_received, total_notes,
        "B missed some notes. Expected {}, got {}",
        total_notes, b_received
    );
    println!("Test A -> B passed for {} notes/sec", rate);

    // Ensure connection is clean logic... (simplified here)
    let _ = client_a.disconnect();
    let _ = client_b.disconnect();
}

#[tokio::test]
#[ignore = "需要本地协作服务器 (localhost:3000)"]
async fn test_sync_1hz() {
    run_sync_test(1, 3).await; // 3 notes takes 3 seconds
}

#[tokio::test]
#[ignore = "需要本地协作服务器 (localhost:3000)"]
async fn test_sync_10hz() {
    run_sync_test(10, 20).await; // 20 notes
}

#[tokio::test]
#[ignore = "需要本地协作服务器 (localhost:3000)"]
async fn test_sync_100hz() {
    run_sync_test(100, 50).await; // 50 notes
}
