use serde::{Deserialize, Serialize};

/// 协作同步音符（与编辑器对应，含会话唯一 ID 与音轨索引）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncNote {
    /// 音符全局唯一 ID（与本地 `NoteEvent.id` 一致，由发送端文档分配器分配）
    pub id: u64,
    /// 起始 tick
    pub tick: f32,
    /// 音高（键号）
    pub key: u16,
    /// 音符时值（长度）
    pub length: f32,
    /// 力度
    pub velocity: u8,
    /// 通道
    pub channel: u8,
    /// 音轨索引
    pub track_index: usize,
}

/// 音符批量操作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteBatchOperation {
    /// 操作类型
    pub action: NoteAction,
    /// 参与操作的音乐符集合
    pub notes: Vec<SyncNote>,
    /// 源音轨索引（移动/复制时使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_track: Option<usize>,
    /// 目标音轨索引（移动/复制时使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_track: Option<usize>,
    /// tick 偏移量（移动时使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_offset: Option<f32>,
    /// 键号偏移量（移动时使用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_offset: Option<i16>,
    /// 操作时间戳
    pub timestamp: u64,
}

/// 音符操作类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteAction {
    /// 添加
    Add,
    /// 更新
    Update,
    /// 删除
    Delete,
    /// 移动
    Move,
    /// 复制
    Copy,
    /// 粘贴
    Paste,
}
