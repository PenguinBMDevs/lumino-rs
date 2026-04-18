use serde::{Deserialize, Serialize};

/// 音符数据结构（与编辑器对应）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub tick: f32,
    pub key: u16,
    pub length: f32,
    pub velocity: u8,
    pub channel: u8,
    pub track_index: usize,
}

/// 音符批量操作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteBatchOperation {
    pub action: NoteAction,
    pub notes: Vec<Note>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_track: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_track: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_offset: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_offset: Option<i16>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteAction {
    Add,
    Update,
    Delete,
    Move,
    Copy,
    Paste,
}
