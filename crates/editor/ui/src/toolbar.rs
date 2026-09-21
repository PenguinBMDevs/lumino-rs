//! Toolbar 模块 - 顶部工具栏组件
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义（NotePrecision, TupletType, DotType, Tool 等）
//! - `event`: 工具栏事件枚举和工厂方法
//! - `view`: 视图渲染逻辑

pub(crate) mod brush_dropdown;
mod buttons;
mod default;
pub mod event;
pub(crate) mod overflow;
mod record;
mod resize;
pub(crate) mod shape_dropdown;
#[cfg(test)]
mod tests;
pub(crate) mod tool_panel;
pub mod types;
mod update;
mod view;

pub use event::{Event, FlipHorizontalMode, ShapeType, ToolPanelItem};
pub use lumino_ui_core::button_descs::ButtonId;
pub use types::{
    CustomPrecisionDialog, DEFAULT_HEIGHT, DotType, MAX_HEIGHT, MIN_HEIGHT, NotePrecision,
    RESIZE_HANDLE_HEIGHT, Tool, TupletType,
};

use lumino_core::BrushConfig;

/// 工具栏视图所需的性能/检测数据聚合
///
/// 移植自 yinhe `chrome/transport_bar.rs` 的 `TransportContext`：用结构体替代长参数列表，
/// 避免工具栏渲染函数参数爆炸。
///
/// 注意：CPU/内存性能数据已移至底部状态栏（statusbar）显示，因此此处不再包含
/// `perf_data`；仅保留时间码换算所需的播放位置/PPQ/速度点。
pub struct ToolbarPerfContext<'a> {
    /// 当前播放位置（tick）
    pub playback_tick: f32,
    /// 每四分音符脉冲数（PPQ）
    pub ppq: u16,
    /// 速度变化点（用于 tick→秒 / BPM 换算）
    pub tempo_points: &'a [lumino_note_core::midi_types::TempoPoint],
}

/// 工具栏组件
pub struct Toolbar {
    /// 当前工具
    pub current_tool: Tool,
    /// 是否正在播放
    pub is_playing: bool,
    /// 是否启用循环播放
    pub is_looping: bool,
    /// 是否正在录制
    pub is_recording: bool,
    /// 工具栏高度（默认 72）
    pub height: f32,
    /// 是否正在拖拽调整高度
    is_resizing: bool,
    /// 拖拽开始时的鼠标 Y 坐标
    resize_start_y: f32,
    /// 拖拽开始时的工具栏高度
    resize_start_height: f32,
    /// 当前音符精度设置
    pub note_precision: NotePrecision,
    /// 音符变速速度因子（浮点值，如 0.5 表示半速）
    pub speed_factor: f32,
    /// Ctrl 键是否按下（用于变速按钮的快捷操作）
    pub ctrl_pressed: bool,
    /// Shift 键是否按下（用于翻转按钮的快捷操作）
    pub shift_pressed: bool,
    /// 自定义精度对话框状态
    pub custom_precision_dialog: CustomPrecisionDialog,
    /// 自动滚动模式
    pub auto_scroll_mode: lumino_core::storage::config::AutoScrollMode,
    /// PPQ 编辑模式（true = 正在编辑）
    pub ppq_editing: bool,
    /// PPQ 编辑缓冲区（仅包含数字字符）
    pub ppq_edit_buffer: String,
    /// 溢出菜单是否打开
    pub overflow_menu_open: bool,
    /// 绘制工具选择面板是否打开（颜料桶右侧小三角触发）
    pub tool_panel_open: bool,
    /// 画刷工具下拉是否打开（ctrl+点击附属按钮触发）
    pub brush_dropdown_open: bool,
    /// 画刷工具配置（粗细度 + 每层音轨分配）
    pub brush: BrushConfig,
    /// 颜料桶填充模式开关（仅曲线工具激活时可操作）
    pub fill_enabled: bool,
    /// 形状工具下拉是否打开（ctrl+点击形状工具触发，隐藏菜单选择矩形/圆形/三角形）
    pub shape_dropdown_open: bool,
    /// 形状工具当前图形类型（矩形/圆形/三角形），由形状工具下拉切换并持久保存
    pub current_shape: ShapeType,
}
