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

/// 项目状态快照
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProjectState {
    /// 音符批量操作列表
    pub notes: Vec<crate::types::NoteBatchOperation>,
    /// 附加字段（透传的其它项目数据）
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}
