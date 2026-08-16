use serde::{Deserialize, Serialize};

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
