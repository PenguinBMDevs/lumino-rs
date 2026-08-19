use serde::{Deserialize, Serialize};

/// 鼠标位置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MousePosition {
    /// X 坐标
    pub x: f32,
    /// Y 坐标
    pub y: f32,
    /// 关联的视图状态（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_state: Option<ViewState>,
}

/// 视图状态（与编辑器状态对应）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    /// 水平滚动偏移
    pub scroll_x: f32,
    /// 垂直滚动偏移
    pub scroll_y: f32,
    /// 水平缩放
    pub zoom_x: f32,
    /// 垂直缩放
    pub zoom_y: f32,
    /// 项目总 tick 数
    pub total_ticks: u32,
    /// 键数量
    pub key_count: u16,
    /// 可见键数量
    pub visible_key_count: u16,
    /// 每拍精度（tick 数）
    pub ppq: u16,
    /// 键盘宽度（像素）
    pub keyboard_width: f32,
    /// 吸附精度
    pub snap_precision: f32,
    /// 默认音符长度
    pub default_note_length: f32,
}
