use serde::{Deserialize, Serialize};

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

/// 项目状态快照
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProjectState {
    pub notes: Vec<crate::types::NoteBatchOperation>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}
