use serde::{Deserialize, Serialize};

/// 项目更新类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    /// 更新类型
    pub update_type: ProjectUpdateType,
    /// 更新数据
    pub data: serde_json::Value,
    /// 更新时间戳
    pub timestamp: u64,
}

/// 项目更新类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectUpdateType {
    /// 视图状态变化
    ViewState,
    /// 音轨变化
    Track,
    /// 速度变化
    Tempo,
    /// 拍号变化
    TimeSignature,
    /// 元数据变化
    Metadata,
    /// 全量更新
    Full,
}

// 2026-10-20（审计 §十一）：删除 `ProjectState`。
// 它是服务端下发的「房间全量状态快照」（含 `notes: Vec<NoteBatchOperation>` 与
// `#[serde(flatten)] extra`），挂在 `ServerMessage::{RoomJoined, FullSync}` 上，
// 但两处消费端都以 `..` 丢弃、`extra` 也从未被读取——即「反序列化后即丢弃」的
// 死负载，且构成房间里第二份音符数据（权威是各端 `MidiDocument`）。
// 字段已在消息类型中移除；服务端继续下发 `projectState` 会被 serde 忽略。
