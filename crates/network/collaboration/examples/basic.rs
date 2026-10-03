use lumino_collaboration::{
    client::CollaborationClient,
    types::{NoteAction, NoteBatchOperation, SyncNote},
};

/// 示例：发送音符操作
#[allow(dead_code)]
async fn send_note_operation_example(
    client: &CollaborationClient,
) -> Result<(), Box<dyn std::error::Error>> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;

    let operation = NoteBatchOperation {
        action: NoteAction::Add,
        notes: vec![
            SyncNote {
                tick: 0.0,
                key: 60,
                length: 960.0,
                velocity: 100,
                channel: 0,
                track_index: 0,
            },
            SyncNote {
                tick: 960.0,
                key: 64,
                length: 960.0,
                velocity: 100,
                channel: 0,
                track_index: 0,
            },
        ],
        source_track: None,
        target_track: None,
        tick_offset: None,
        key_offset: None,
        timestamp,
    };

    client.send_note_batch(operation)?;
    Ok(())
}

fn main() {
    println!("Example compiled successfully.");
}
