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
