use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

use lumino_collaboration::{
    client::{CollaborationClient, CollaborationEvent},
    types::{ClientConfig, NoteBatchOperation, NoteAction, Note},
};

// 工具函数：创建客户端并等待连接和认证
async fn setup_client(username: &str) -> (CollaborationClient, mpsc::UnboundedReceiver<CollaborationEvent>) {
    let config = ClientConfig {
        server_host: "localhost".to_string(),
        server_port: 3000,
        username: username.to_string(),
        auto_reconnect: false,
        max_reconnect_attempts: 1,
    };

    let mut client = CollaborationClient::new(config);
    let (tx, rx) = mpsc::unbounded_channel();

    client.set_event_callback(move |event| {
        let _ = tx.send(event);
    });

    client.connect(None, None).await.expect("Failed to connect");

    (client, rx)
}

async fn wait_for_auth(rx: &mut mpsc::UnboundedReceiver<CollaborationEvent>) -> (String, String) {
    println!("Waiting for auth...");
    while let Some(event) = rx.recv().await {
        println!("Got event during auth: {:?}", event);
        if let CollaborationEvent::Authenticated { user_id, invite_code } = event {
            return (user_id, invite_code);
        }
    }
    panic!("Failed to authenticate");
}

async fn run_sync_test(rate: u64, total_notes: u32) {
    println!("Setting up clients...");
    let (mut client_a, mut rx_a) = setup_client("TestA").await;
    let (mut client_b, mut rx_b) = setup_client("TestB").await;

    let (a_id, a_invite) = wait_for_auth(&mut rx_a).await;
    let (_b_id, _b_invite) = wait_for_auth(&mut rx_b).await;

    println!("A creating room...");
    // A create room
    client_a.create_room("TestRoom".to_string()).unwrap();

    // Wait for A to create room
    let mut room_created = false;
    let mut room_invite_code = String::new();
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(100), rx_a.recv()).await {
            println!("A event: {:?}", event);
            if let CollaborationEvent::RoomCreated { room } = event {
                room_created = true;
                room_invite_code = room.invite_code.clone();
                break;
            }
        }
    }
    assert!(room_created, "Room should be created in A");

    // B join room
    client_b.join_room(room_invite_code).unwrap();

    // Wait for B to join room
    let mut joined = false;
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(Some(event)) = tokio::time::timeout(Duration::from_millis(100), rx_b.recv()).await {
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
            notes: vec![Note {
                id: format!("note_{}_{}", rate, i),
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
        client_a.send_note_batch(op).unwrap();
        sleep(delay).await;
    }

    // Check if B received all
    let mut b_received = 0;
    let timeout = Duration::from_secs(3);
    let start = std::time::Instant::now();

    while b_received < total_notes && start.elapsed() < timeout {
        if let Ok(event) = tokio::time::timeout(Duration::from_millis(100), rx_b.recv()).await {
            if let Some(CollaborationEvent::NoteBatch { .. }) = event {
                b_received += 1;
            }
        }
    }

    assert_eq!(b_received, total_notes, "B missed some notes. Expected {}, got {}", total_notes, b_received);
    println!("Test A -> B passed for {} notes/sec", rate);

    // Ensure connection is clean logic... (simplified here)
    client_a.disconnect().await;
    client_b.disconnect().await;
}

#[tokio::test]
async fn test_sync_1hz() {
    run_sync_test(1, 3).await; // 3 notes takes 3 seconds
}

#[tokio::test]
async fn test_sync_10hz() {
    run_sync_test(10, 20).await; // 20 notes
}

#[tokio::test]
async fn test_sync_100hz() {
    run_sync_test(100, 50).await; // 50 notes
}
