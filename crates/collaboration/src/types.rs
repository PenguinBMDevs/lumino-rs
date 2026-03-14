use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 用户颜色（16进制字符串）
pub type UserColor = String;

/// 用户ID
pub type UserId = String;

/// 房间ID
pub type RoomId = String;

/// 邀请码
pub type InviteCode = String;

/// Socket ID
pub type SocketId = String;

/// 鼠标位置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MousePosition {
    pub x: f32,
    pub y: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_state: Option<ViewState>,
}

/// 视图状态（与编辑器状态对应）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub total_ticks: u32,
    pub key_count: u16,
    pub visible_key_count: u16,
    pub ppq: u16,
    pub keyboard_width: f32,
    pub snap_precision: f32,
    pub default_note_length: f32,
}

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    pub id: UserId,
    pub username: String,
    pub color: UserColor,
    pub is_host: bool,
}

/// 房间信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo {
    pub id: RoomId,
    pub invite_code: InviteCode,
    pub name: String,
    pub host_id: UserId,
    pub user_count: usize,
    pub max_users: usize,
}

/// 远程用户（包含实时信息）
#[derive(Debug, Clone)]
pub struct RemoteUser {
    pub info: UserInfo,
    pub mouse_position: Option<MousePosition>,
    pub last_active: std::time::Instant,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteAction {
    Add,
    Update,
    Delete,
    Move,
    Copy,
    Paste,
}

/// MIDI事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MidiEvent {
    NoteOn {
        track: usize,
        tick: u32,
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOff {
        track: usize,
        tick: u32,
        channel: u8,
        key: u8,
        velocity: u8,
    },
    ControlChange {
        track: usize,
        tick: u32,
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        track: usize,
        tick: u32,
        channel: u8,
        program: u8,
    },
    Tempo {
        track: usize,
        tick: u32,
        tempo: u32,
    },
    TimeSignature {
        track: usize,
        tick: u32,
        numerator: u8,
        denominator: u8,
    },
    KeySignature {
        track: usize,
        tick: u32,
        key: i8,
        #[serde(rename = "isMajor")]
        is_major: bool,
    },
    TrackName {
        track: usize,
        tick: u32,
        name: String,
    },
    Other {
        track: usize,
        tick: u32,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        raw: Vec<u8>,
    },
}

impl From<&lumino_core::MidiEvent> for MidiEvent {
    fn from(event: &lumino_core::MidiEvent) -> Self {
        use lumino_core::MidiEvent as CoreEvent;

        match event {
            CoreEvent::NoteOn {
                track,
                tick,
                channel,
                key,
                velocity,
            } => Self::NoteOn {
                track: *track,
                tick: *tick,
                channel: *channel,
                key: *key,
                velocity: *velocity,
            },
            CoreEvent::NoteOff {
                track,
                tick,
                channel,
                key,
                velocity,
            } => Self::NoteOff {
                track: *track,
                tick: *tick,
                channel: *channel,
                key: *key,
                velocity: *velocity,
            },
            CoreEvent::ControlChange {
                track,
                tick,
                channel,
                controller,
                value,
            } => Self::ControlChange {
                track: *track,
                tick: *tick,
                channel: *channel,
                controller: *controller,
                value: *value,
            },
            CoreEvent::ProgramChange {
                track,
                tick,
                channel,
                program,
            } => Self::ProgramChange {
                track: *track,
                tick: *tick,
                channel: *channel,
                program: *program,
            },
            CoreEvent::Tempo { track, tick, tempo } => Self::Tempo {
                track: *track,
                tick: *tick,
                tempo: *tempo,
            },
            CoreEvent::TimeSignature {
                track,
                tick,
                numerator,
                denominator,
            } => Self::TimeSignature {
                track: *track,
                tick: *tick,
                numerator: *numerator,
                denominator: *denominator,
            },
            CoreEvent::KeySignature {
                track,
                tick,
                key,
                is_major,
            } => Self::KeySignature {
                track: *track,
                tick: *tick,
                key: *key,
                is_major: *is_major,
            },
            CoreEvent::TrackName { track, tick, name } => Self::TrackName {
                track: *track,
                tick: *tick,
                name: name.clone(),
            },
            CoreEvent::Other { track, tick, raw } => Self::Other {
                track: *track,
                tick: *tick,
                raw: raw.clone(),
            },
        }
    }
}

/// 项目更新类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub update_type: ProjectUpdateType,
    pub data: serde_json::Value,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectUpdateType {
    ViewState,
    Track,
    Tempo,
    TimeSignature,
    Metadata,
    Full,
}

/// 客户端状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    Authenticating,
    Authenticated,
    InRoom,
    Error,
}

/// 协作会话状态
#[derive(Debug, Clone, Default)]
pub struct CollaborationSession {
    pub current_room: Option<RoomInfo>,
    pub remote_users: HashMap<UserId, RemoteUser>,
    pub current_user_id: Option<UserId>,
    pub invite_code: Option<InviteCode>,
}

/// 客户端配置
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub server_host: String,
    pub server_port: u16,
    pub username: String,
    pub auto_reconnect: bool,
    pub max_reconnect_attempts: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u32;

        Self {
            server_host: "localhost".to_string(),
            server_port: 3000,
            username: format!("用户{}", seed % 10000),
            auto_reconnect: true,
            max_reconnect_attempts: 5,
        }
    }
}
