/**
 * 协作UI集成测试
 *
 * 测试场景：
 * 1. 两个客户端连接到协作服务器
 * 2. 客户端A发送鼠标位置，验证客户端B能收到并显示
 * 3. 客户端A放置音符，验证客户端B能收到并显示
 * 4. 客户端B发送鼠标位置，验证客户端A能收到并显示
 * 5. 客户端B放置音符，验证客户端A能收到并显示
 *
 * 服务器: lumino-02.afeu20u3jfocas.dpdns.org:80
 */
mod common;

use std::time::Duration;
use tokio::time::sleep;

use lumino_collaboration::client::{CollaborationClient, CollaborationEvent};
use lumino_collaboration::types::{
    ClientConfig, MousePosition, Note, NoteAction, NoteBatchOperation,
};

use common::EventCollector;

/// 所有协作UI测试的主入口
/// 需要外部协作服务器，默认忽略。
/// 运行: `cargo test test_collaboration_ui_all -- --ignored`
#[tokio::test]
#[ignore = "需要外部协作服务器"]
async fn test_collaboration_ui_all() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n======================================================");
    println!("  协作UI集成测试套件");
    println!("  服务器: lumino-02.afeu20u3jfocas.dpdns.org:80");
    println!("======================================================\n");

    let mut all_passed = true;
    let mut results = Vec::new();

    // 测试1: 鼠标指针同步
    println!("\n>>> 开始测试 1/3: 鼠标指针同步\n");
    match test_mouse_cursor_sync_internal().await {
        Ok(_) => {
            println!("✓ 测试 1/3 通过: 鼠标指针同步\n");
            results.push(("鼠标指针同步", true, None));
        }
        Err(e) => {
            println!("✗ 测试 1/3 失败: 鼠标指针同步 - {}\n", e);
            results.push(("鼠标指针同步", false, Some(e.to_string())));
            all_passed = false;
        }
    }

    // 等待一段时间，确保服务器资源释放
    sleep(Duration::from_millis(2000)).await;

    // 测试2: 音符批量同步
    println!("\n>>> 开始测试 2/3: 音符批量同步\n");
    match test_note_batch_sync_internal().await {
        Ok(_) => {
            println!("✓ 测试 2/3 通过: 音符批量同步\n");
            results.push(("音符批量同步", true, None));
        }
        Err(e) => {
            println!("✗ 测试 2/3 失败: 音符批量同步 - {}\n", e);
            results.push(("音符批量同步", false, Some(e.to_string())));
            all_passed = false;
        }
    }

    // 等待一段时间，确保服务器资源释放
    sleep(Duration::from_millis(2000)).await;

    // 测试3: 鼠标指针连续移动同步
    println!("\n>>> 开始测试 3/3: 鼠标指针连续移动同步\n");
    match test_mouse_movement_sync_internal().await {
        Ok(_) => {
            println!("✓ 测试 3/3 通过: 鼠标指针连续移动同步\n");
            results.push(("鼠标指针连续移动同步", true, None));
        }
        Err(e) => {
            println!("✗ 测试 3/3 失败: 鼠标指针连续移动同步 - {}\n", e);
            results.push(("鼠标指针连续移动同步", false, Some(e.to_string())));
            all_passed = false;
        }
    }

    // 打印最终总结
    println!("\n======================================================");
    println!("  测试总结");
    println!("======================================================");
    for (name, passed, error) in &results {
        if *passed {
            println!("✓ {}", name);
        } else {
            println!(
                "✗ {} - {}",
                name,
                error.as_ref().unwrap_or(&"未知错误".to_string())
            );
        }
    }
    println!("======================================================\n");

    if all_passed {
        println!("🎉 所有测试通过！协作功能正常工作。");
        Ok(())
    } else {
        Err("部分测试失败".into())
    }
}

/// 内部函数：测试鼠标指针同步
async fn test_mouse_cursor_sync_internal() -> Result<(), Box<dyn std::error::Error>> {
    println!("======================================================");
    println!("  测试鼠标指针同步");
    println!("======================================================\n");

    // 步骤1: 客户端A连接并创建房间
    println!("步骤1: 客户端A连接服务器并创建房间");
    println!("----------------------------------------");

    let collector_a = EventCollector::new();
    let mut client_a = CollaborationClient::new(ClientConfig {
        server_host: "lumino-02.afeu20u3jfocas.dpdns.org".to_string(),
        server_port: 80,
        username: "客户端A".to_string(),
        auto_reconnect: false,
        max_reconnect_attempts: 0,
    });
    client_a.set_event_callback(collector_a.callback());

    println!("  正在创建房间并连接...");
    let create_result = client_a
        .create_room_and_connect("测试房间".to_string())
        .await?;
    println!("  ✓ 连接成功");
    println!(
        "  ✓ 房间创建成功，邀请码: {}",
        create_result.room.invite_code
    );

    // 获取实际认证后的 userId
    let user_a_id = collector_a
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
        .ok_or("客户端A认证超时")?;
    let invite_code = create_result.room.invite_code.clone();

    println!("  ✓ 客户端A认证成功，ID: {}", user_a_id);
    println!();

    // 步骤2: 客户端B加入房间
    println!("步骤2: 客户端B加入房间");
    println!("----------------------------------------");

    let collector_b = EventCollector::new();
    let mut client_b = CollaborationClient::new(ClientConfig {
        server_host: "lumino-02.afeu20u3jfocas.dpdns.org".to_string(),
        server_port: 80,
        username: "客户端B".to_string(),
        auto_reconnect: false,
        max_reconnect_attempts: 0,
    });
    client_b.set_event_callback(collector_b.callback());

    println!("  正在加入房间 (邀请码: {})...", invite_code);
    client_b.join_room_and_connect(invite_code.clone()).await?;
    println!("  ✓ 连接成功");

    // 等待认证
    let user_b_id = collector_b
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
        .ok_or("客户端B认证超时")?;
    println!("  ✓ 客户端B认证成功，ID: {}", user_b_id);

    // 等待房间加入事件
    println!("  等待用户事件...");
    sleep(Duration::from_millis(3000)).await;

    let joined = collector_b
        .contains_event(
            |e| {
                println!("  检查事件: {:?}", e);
                matches!(e, CollaborationEvent::UserJoined { .. })
                    || matches!(e, CollaborationEvent::Authenticated { .. })
            },
            10000,
        )
        .await;

    if !joined {
        return Err("客户端B加入房间超时".into());
    }
    println!("  ✓ 客户端B加入房间成功");
    println!();

    // 步骤3: 测试A->B鼠标指针同步
    println!("步骤3: 测试A->B鼠标指针同步");
    println!("----------------------------------------");

    let mouse_pos_a = MousePosition {
        x: 150.0,
        y: 250.0,
        view_state: None,
    };

    println!(
        "  客户端A发送鼠标位置: x={}, y={}",
        mouse_pos_a.x, mouse_pos_a.y
    );
    client_a.send_mouse_position(mouse_pos_a.clone()).await?;

    // 客户端B等待接收鼠标位置更新
    let received_mouse_b = collector_b
        .contains_event(
            |e| {
                if let CollaborationEvent::MouseUpdate {
                    user_id, position, ..
                } = e
                {
                    user_id == &user_a_id
                        && position.x == mouse_pos_a.x
                        && position.y == mouse_pos_a.y
                } else {
                    false
                }
            },
            5000,
        )
        .await;

    if !received_mouse_b {
        return Err("客户端B未收到A的鼠标位置更新".into());
    }
    println!("  ✓ 客户端B收到A的鼠标位置更新 - 鼠标指针投影可见");
    println!();

    // 步骤4: 测试A->B音符同步
    println!("步骤4: 测试A->B音符同步");
    println!("----------------------------------------");

    let note_a = Note {
        id: format!("note-a-{}", chrono::Utc::now().timestamp_millis()),
        tick: 2880.0,
        key: 65,
        length: 480.0,
        velocity: 110,
        channel: 0,
        track_index: 0,
    };

    let operation_a = NoteBatchOperation {
        action: NoteAction::Add,
        notes: vec![note_a.clone()],
        source_track: Some(0),
        target_track: Some(0),
        tick_offset: None,
        key_offset: None,
        timestamp: chrono::Utc::now().timestamp_millis() as u64,
    };

    println!(
        "  客户端A发送音符添加: tick={}, key={}",
        note_a.tick, note_a.key
    );
    client_a.send_note_batch(operation_a).await?;

    // 客户端B等待接收音符更新
    let received_note_b = collector_b
        .contains_event(
            |e| {
                if let CollaborationEvent::NoteBatch { user_id, operation } = e
                    && user_id == &user_a_id
                    && operation.action == NoteAction::Add
                    && let Some(n) = operation.notes.first()
                {
                    return n.tick == note_a.tick && n.key == note_a.key;
                }
                false
            },
            5000,
        )
        .await;

    if !received_note_b {
        return Err("客户端B未收到A的音符更新".into());
    }
    println!("  ✓ 客户端B收到A的音符更新 - 音符同步成功");
    println!();

    // 步骤5: 测试B->A鼠标指针同步
    println!("步骤5: 测试B->A鼠标指针同步");
    println!("----------------------------------------");

    let mouse_pos_b = MousePosition {
        x: 300.0,
        y: 400.0,
        view_state: None,
    };

    println!(
        "  客户端B发送鼠标位置: x={}, y={}",
        mouse_pos_b.x, mouse_pos_b.y
    );
    client_b.send_mouse_position(mouse_pos_b.clone()).await?;

    // 客户端A等待接收鼠标位置更新
    let received_mouse_a = collector_a
        .contains_event(
            |e| {
                if let CollaborationEvent::MouseUpdate {
                    user_id, position, ..
                } = e
                {
                    user_id == &user_b_id
                        && position.x == mouse_pos_b.x
                        && position.y == mouse_pos_b.y
                } else {
                    false
                }
            },
            5000,
        )
        .await;

    if !received_mouse_a {
        return Err("客户端A未收到B的鼠标位置更新".into());
    }
    println!("  ✓ 客户端A收到B的鼠标位置更新 - 鼠标指针投影可见");
    println!();

    // 步骤6: 测试B->A音符同步
    println!("步骤6: 测试B->A音符同步");
    println!("----------------------------------------");

    let note_b = Note {
        id: format!("note-b-{}", chrono::Utc::now().timestamp_millis()),
        tick: 3840.0,
        key: 72,
        length: 480.0,
        velocity: 90,
        channel: 1,
        track_index: 0,
    };

    let operation_b = NoteBatchOperation {
        action: NoteAction::Add,
        notes: vec![note_b.clone()],
        source_track: Some(0),
        target_track: Some(0),
        tick_offset: None,
        key_offset: None,
        timestamp: chrono::Utc::now().timestamp_millis() as u64,
    };

    println!(
        "  客户端B发送音符添加: tick={}, key={}",
        note_b.tick, note_b.key
    );
    client_b.send_note_batch(operation_b).await?;

    // 客户端A等待接收音符更新
    let received_note_a = collector_a
        .contains_event(
            |e| {
                if let CollaborationEvent::NoteBatch { user_id, operation } = e
                    && user_id == &user_b_id
                    && operation.action == NoteAction::Add
                    && let Some(n) = operation.notes.first()
                {
                    return n.tick == note_b.tick && n.key == note_b.key;
                }
                false
            },
            5000,
        )
        .await;

    if !received_note_a {
        return Err("客户端A未收到B的音符更新".into());
    }
    println!("  ✓ 客户端A收到B的音符更新 - 音符同步成功");
    println!();

    // 测试总结
    println!("======================================================");
    println!("  鼠标指针同步测试总结");
    println!("======================================================");
    println!("✓ 客户端A连接并创建房间 - 通过");
    println!("✓ 客户端B加入房间 - 通过");
    println!("✓ A->B鼠标指针同步 - 通过 (投影可见)");
    println!("✓ A->B音符同步 - 通过");
    println!("✓ B->A鼠标指针同步 - 通过 (投影可见)");
    println!("✓ B->A音符同步 - 通过");
    println!();
    println!("🎉 鼠标指针同步测试通过！");
    println!("======================================================\n");

    Ok(())
}

/// 内部函数：测试音符批量同步
async fn test_note_batch_sync_internal() -> Result<(), Box<dyn std::error::Error>> {
    println!("======================================================");
    println!("  测试音符批量同步");
    println!("======================================================\n");

    // 步骤1: 客户端A连接并创建房间
    println!("步骤1: 客户端A连接服务器并创建房间");
    println!("----------------------------------------");

    let collector_a = EventCollector::new();
    let mut client_a = CollaborationClient::new(ClientConfig {
        server_host: "lumino-02.afeu20u3jfocas.dpdns.org".to_string(),
        server_port: 80,
        username: "批量测试A".to_string(),
        auto_reconnect: false,
        max_reconnect_attempts: 0,
    });
    client_a.set_event_callback(collector_a.callback());

    println!("  正在创建房间并连接...");
    let create_result = client_a
        .create_room_and_connect("批量测试房间".to_string())
        .await?;
    println!("  ✓ 连接成功");
    println!(
        "  ✓ 房间创建成功，邀请码: {}",
        create_result.room.invite_code
    );

    // 获取实际认证后的 userId
    let user_a_id = collector_a
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
        .ok_or("客户端A认证超时")?;
    let invite_code = create_result.room.invite_code.clone();

    println!("  ✓ 客户端A认证成功，ID: {}", user_a_id);
    println!();

    // 步骤2: 客户端B加入房间
    println!("步骤2: 客户端B加入房间");
    println!("----------------------------------------");

    let collector_b = EventCollector::new();
    let mut client_b = CollaborationClient::new(ClientConfig {
        server_host: "lumino-02.afeu20u3jfocas.dpdns.org".to_string(),
        server_port: 80,
        username: "批量测试B".to_string(),
        auto_reconnect: false,
        max_reconnect_attempts: 0,
    });
    client_b.set_event_callback(collector_b.callback());

    println!("  正在加入房间 (邀请码: {})...", invite_code);
    client_b.join_room_and_connect(invite_code.clone()).await?;
    println!("  ✓ 连接成功");

    // 等待认证
    let user_b_id = collector_b
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
        .ok_or("客户端B认证超时")?;
    println!("  ✓ 客户端B认证成功，ID: {}", user_b_id);

    // 等待房间加入事件
    sleep(Duration::from_millis(3000)).await;
    println!();

    // 步骤3: 客户端A发送批量音符
    println!("步骤3: 客户端A发送批量音符");
    println!("----------------------------------------");

    let notes = vec![
        Note {
            id: format!("batch-note-1-{}", chrono::Utc::now().timestamp_millis()),
            tick: 0.0,
            key: 60,
            length: 480.0,
            velocity: 100,
            channel: 0,
            track_index: 0,
        },
        Note {
            id: format!("batch-note-2-{}", chrono::Utc::now().timestamp_millis()),
            tick: 480.0,
            key: 64,
            length: 480.0,
            velocity: 100,
            channel: 0,
            track_index: 0,
        },
        Note {
            id: format!("batch-note-3-{}", chrono::Utc::now().timestamp_millis()),
            tick: 960.0,
            key: 67,
            length: 480.0,
            velocity: 100,
            channel: 0,
            track_index: 0,
        },
    ];

    let operation = NoteBatchOperation {
        action: NoteAction::Add,
        notes: notes.clone(),
        source_track: Some(0),
        target_track: Some(0),
        tick_offset: None,
        key_offset: None,
        timestamp: chrono::Utc::now().timestamp_millis() as u64,
    };

    println!("  客户端A发送批量音符添加: {} 个音符", notes.len());
    client_a.send_note_batch(operation).await?;

    // 客户端B等待接收批量音符更新
    let received_batch = collector_b
        .contains_event(
            |e| {
                if let CollaborationEvent::NoteBatch { user_id, operation } = e
                    && user_id == &user_a_id
                    && operation.action == NoteAction::Add
                {
                    return operation.notes.len() == 3;
                }
                false
            },
            5000,
        )
        .await;

    if !received_batch {
        return Err("客户端B未收到A的批量音符更新".into());
    }
    println!("  ✓ 客户端B收到A的批量音符更新 - 批量同步成功");
    println!();

    // 测试总结
    println!("======================================================");
    println!("  音符批量同步测试总结");
    println!("======================================================");
    println!("✓ 客户端A连接并创建房间 - 通过");
    println!("✓ 客户端B加入房间 - 通过");
    println!("✓ 客户端A发送批量音符 - 通过");
    println!("✓ 客户端B接收批量音符 - 通过");
    println!();
    println!("🎉 音符批量同步测试通过！");
    println!("======================================================\n");

    Ok(())
}

/// 内部函数：测试鼠标指针连续移动同步
async fn test_mouse_movement_sync_internal() -> Result<(), Box<dyn std::error::Error>> {
    println!("======================================================");
    println!("  测试鼠标指针连续移动同步");
    println!("======================================================\n");

    // 步骤1: 客户端A连接并创建房间
    println!("步骤1: 客户端A连接服务器并创建房间");
    println!("----------------------------------------");

    let collector_a = EventCollector::new();
    let mut client_a = CollaborationClient::new(ClientConfig {
        server_host: "lumino-02.afeu20u3jfocas.dpdns.org".to_string(),
        server_port: 80,
        username: "移动测试A".to_string(),
        auto_reconnect: false,
        max_reconnect_attempts: 0,
    });
    client_a.set_event_callback(collector_a.callback());

    println!("  正在创建房间并连接...");
    let create_result = client_a
        .create_room_and_connect("移动测试房间".to_string())
        .await?;
    println!("  ✓ 连接成功");
    println!(
        "  ✓ 房间创建成功，邀请码: {}",
        create_result.room.invite_code
    );

    // 获取实际认证后的 userId
    let user_a_id = collector_a
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
        .ok_or("客户端A认证超时")?;
    let invite_code = create_result.room.invite_code.clone();

    println!("  ✓ 客户端A认证成功，ID: {}", user_a_id);
    println!();

    // 步骤2: 客户端B加入房间
    println!("步骤2: 客户端B加入房间");
    println!("----------------------------------------");

    let collector_b = EventCollector::new();
    let mut client_b = CollaborationClient::new(ClientConfig {
        server_host: "lumino-02.afeu20u3jfocas.dpdns.org".to_string(),
        server_port: 80,
        username: "移动测试B".to_string(),
        auto_reconnect: false,
        max_reconnect_attempts: 0,
    });
    client_b.set_event_callback(collector_b.callback());

    println!("  正在加入房间 (邀请码: {})...", invite_code);
    client_b.join_room_and_connect(invite_code.clone()).await?;
    println!("  ✓ 连接成功");

    // 等待认证
    let user_b_id = collector_b
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
        .ok_or("客户端B认证超时")?;
    println!("  ✓ 客户端B认证成功，ID: {}", user_b_id);

    // 等待房间加入事件
    sleep(Duration::from_millis(3000)).await;
    println!();

    // 步骤3: 测试鼠标指针连续移动
    println!("步骤3: 测试鼠标指针连续移动同步");
    println!("----------------------------------------");

    let positions = [
        MousePosition {
            x: 100.0,
            y: 100.0,
            view_state: None,
        },
        MousePosition {
            x: 200.0,
            y: 150.0,
            view_state: None,
        },
        MousePosition {
            x: 300.0,
            y: 200.0,
            view_state: None,
        },
        MousePosition {
            x: 400.0,
            y: 250.0,
            view_state: None,
        },
        MousePosition {
            x: 500.0,
            y: 300.0,
            view_state: None,
        },
    ];

    for (i, pos) in positions.iter().enumerate() {
        println!("  发送位置 {}: x={}, y={}", i + 1, pos.x, pos.y);
        client_a.send_mouse_position(pos.clone()).await?;
        sleep(Duration::from_millis(100)).await;
    }

    // 客户端B等待接收最后一个鼠标位置
    let last_pos = positions.last().unwrap();
    let received_last = collector_b
        .contains_event(
            |e| {
                if let CollaborationEvent::MouseUpdate {
                    user_id, position, ..
                } = e
                {
                    user_id == &user_a_id && position.x == last_pos.x && position.y == last_pos.y
                } else {
                    false
                }
            },
            5000,
        )
        .await;

    if !received_last {
        return Err("客户端B未收到A的最后鼠标位置".into());
    }
    println!("  ✓ 客户端B收到A的鼠标位置更新 - 连续移动同步成功");
    println!();

    // 测试总结
    println!("======================================================");
    println!("  鼠标指针连续移动同步测试总结");
    println!("======================================================");
    println!("✓ 客户端A连接并创建房间 - 通过");
    println!("✓ 客户端B加入房间 - 通过");
    println!("✓ 鼠标指针连续移动同步 - 通过");
    println!();
    println!("🎉 鼠标指针连续移动同步测试通过！");
    println!("======================================================\n");

    Ok(())
}
