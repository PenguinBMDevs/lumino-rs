use serde::{Deserialize, Serialize};

/// `UiState` 内几何字段（`w/h/x/y`）的口径版本
///
/// - `0`：历史文件，`w/h/x/y` 是**物理像素**（写入侧用 `PhysicalSize`/`PhysicalPosition`）
/// - `2`：逻辑像素（REF-001 阶段 G-2）。写入与读取同为逻辑口径，
///   因此换 DPI 显示器/重启不再把窗口越放越大。
///
/// 缺字段时 serde 落 `0` ⇒ 老文件被识别为待迁移，见
/// `crate::runner::window_manager::geometry::migrate_units`。
pub const UI_STATE_LAYOUT_VERSION: u32 = 2;

/// 用户界面状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiState {
    /// 窗口左上角 X 坐标（None 表示未定位，由系统决定）
    pub x: Option<i32>,
    /// 窗口左上角 Y 坐标（None 表示未定位，由系统决定）
    pub y: Option<i32>,
    /// 窗口宽度（**逻辑**像素，见 [`UI_STATE_LAYOUT_VERSION`]）
    pub w: u32,
    /// 窗口高度（**逻辑**像素，见 [`UI_STATE_LAYOUT_VERSION`]）
    pub h: u32,
    /// 窗口是否处于最大化状态
    pub is_maximized: bool,
    /// 几何字段口径版本（缺省 = 0 = 历史物理像素文件）
    #[serde(default)]
    pub version: u32,
}

/// 用户界面状态默认值
impl Default for UiState {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            w: 1440,             // 默认宽度1440逻辑像素
            h: 900,              // 默认高度900逻辑像素
            is_maximized: false, // 默认不是最大化状态
            version: UI_STATE_LAYOUT_VERSION,
        }
    }
}
