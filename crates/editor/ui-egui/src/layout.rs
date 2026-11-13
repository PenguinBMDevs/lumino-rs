//! 与生产 旧版 UI 对齐的逻辑尺寸常量。
//!
//! 来源：
//! - `crates/editor/ui/src/titlebar.rs`：标题栏 30
//! - `crates/editor/ui/src/toolbar/types.rs`：工具栏 72（内容 66 + 手柄 6）
//! - `crates/editor/ui/src/statusbar.rs`：状态栏 20
//! - `crates/editor/ui/src/sidebar/core/types.rs`：路由栏 48 / 面板 200
//! - `crates/editor/ui/src/right_sidebar/core.rs`：路由栏 48
//! - `crates/core/core/src/view_state.rs`：键盘 120 / 标尺 24 / 键高 20
//! - `crates/editor/ui-settings/src/components/constants.rs`：菜单 200 / 内容内边距 20

/// 主窗口标题栏高度。
pub const TITLEBAR_H: f32 = 30.0;
/// 工具栏总高度。
pub const TOOLBAR_H: f32 = 72.0;
/// 工具栏拖拽手柄高度。
pub const TOOLBAR_RESIZE_H: f32 = 6.0;
/// 工具栏内容高度。
pub const TOOLBAR_CONTENT_H: f32 = TOOLBAR_H - TOOLBAR_RESIZE_H;
/// 状态栏高度。
pub const STATUSBAR_H: f32 = 20.0;

/// 左侧 / 右侧路由栏宽度。
pub const ROUTE_BAR_W: f32 = 48.0;
/// 左侧栏内容面板宽度。
pub const SIDEBAR_PANEL_W: f32 = 200.0;
/// 左侧栏总宽度。
pub const SIDEBAR_W: f32 = ROUTE_BAR_W + SIDEBAR_PANEL_W;
/// 右侧栏总宽度（面板默认收起，仅路由栏）。
pub const RIGHT_SIDEBAR_W: f32 = ROUTE_BAR_W;

/// 钢琴键盘宽度。
pub const KEYBOARD_W: f32 = 120.0;
/// 时间标尺高度。
pub const RULER_H: f32 = 24.0;
/// 每个琴键的高度。
pub const KEY_H: f32 = 20.0;
/// 滚动条厚度（旧版 `V_SCROLLBAR_WIDTH` / `H_SCROLLBAR_HEIGHT`）。
pub const SCROLLBAR: f32 = 20.0;

/// 工具栏内容区左 / 右内边距。
pub const TOOLBAR_PAD_X: f32 = 16.0;
/// 工具栏内容区上 / 下内边距。
pub const TOOLBAR_PAD_Y: f32 = 8.0;
/// 工具栏分组容器高度（66 - 上下内边距 16）。
pub const TOOLBAR_GROUP_H: f32 = TOOLBAR_CONTENT_H - TOOLBAR_PAD_Y * 2.0;
/// 工具栏分组圆角。
pub const TOOLBAR_GROUP_RADIUS: u8 = 4;
/// 录制分组宽度。
pub const RECORD_GROUP_W: f32 = 56.0;
/// 播放控制分组宽度。
pub const PLAYBACK_GROUP_W: f32 = 132.0;
/// 循环切换分组宽度。
pub const LOOP_GROUP_W: f32 = 40.0;
/// 撤销 / 重做分组宽度。
pub const UNDO_REDO_GROUP_W: f32 = 96.0;
/// 检测仪表盘分组宽度。
pub const DASHBOARD_GROUP_W: f32 = 200.0;
/// 仪表盘第一格宽度。
pub const DASHBOARD_CELL_W: f32 = 116.0;
/// 右侧工具按钮宽度。
pub const RIGHT_TOOL_W: f32 = 32.0;
/// 右侧工具按钮间距。
pub const RIGHT_TOOL_GAP: f32 = 16.0;
/// 「更多」按钮宽度。
pub const MORE_BUTTON_W: f32 = 28.0;
/// 精度选择器宽度。
/// 设置页输入框高度（生产 `text_input` 31）。
pub const SETTINGS_INPUT_H: f32 = 31.0;
/// 设置页下拉框高度（生产 `pick_list` 32）。
pub const SETTINGS_COMBO_H: f32 = 32.0;

/// 精度下拉宽度（生产：16px 图标 + 6 + 13px 文本 + 左右内边距 8 + 1px 边框）。
pub const PRECISION_W: f32 = 57.0;
/// 精度下拉高度（生产按钮内容高 24 + 上下边框）。
pub const PRECISION_H: f32 = 26.0;

/// 工具栏工具按钮图标尺寸。
pub const TOOL_BUTTON_ICON: f32 = 20.0;
/// 工具选择器图标尺寸。
pub const TOOL_SELECTOR_ICON: f32 = 17.0;
/// 工具按钮内边距。
pub const TOOL_BUTTON_PADDING: f32 = 4.0;
/// 普通工具按钮宽度。
pub const TOOL_BUTTON_W: f32 = TOOL_BUTTON_ICON + TOOL_BUTTON_PADDING * 2.0;
/// 工具选择器按钮宽度。
pub const TOOL_SELECTOR_W: f32 = TOOL_SELECTOR_ICON + TOOL_BUTTON_PADDING * 2.0;

/// 标题栏窗口按钮宽度（旧版 45×29）。
pub const TRAFFIC_BTN_W: f32 = 45.0;
/// 标题栏窗口按钮高度。
pub const TRAFFIC_BTN_H: f32 = 29.0;
/// 窗口按钮图标尺寸。
pub const TRAFFIC_ICON: f32 = 10.0;

/// 设置页左侧菜单宽度。
pub const SETTINGS_MENU_W: f32 = 200.0;
/// 设置页内容内边距。
pub const SETTINGS_PADDING: f32 = 20.0;
/// 设置菜单项高度（旧版 基线逐行间距 52）。
pub const SETTINGS_MENU_ROW_H: f32 = 52.0;
/// 设置页内容外边距。
pub const SETTINGS_MARGIN: f32 = 16.0;
/// 设置页确认按钮宽度。
pub const SETTINGS_CONFIRM_W: f32 = 100.0;
/// 设置页确认按钮高度。
pub const SETTINGS_CONFIRM_H: f32 = 34.0;
/// 设置页确认按钮与容器的间距。
pub const SETTINGS_CONFIRM_GAP: f32 = 16.0;
/// 设置表单行高。
pub const SETTINGS_FORM_ROW_H: f32 = 72.0;

/// 左侧栏拖拽手柄宽度（旧版 `RESIZE_HANDLE_WIDTH`）。
pub const SIDEBAR_RESIZE_W: f32 = 6.0;
/// 音轨列表滚动条槽宽度（旧版 `scrollable` 的 8px）。
pub const SIDEBAR_SCROLLBAR_W: f32 = 8.0;
/// 音轨行高（14px 文本行高 17 + 行内边距 8 + 按钮内边距 10）。
pub const TRACK_ROW_H: f32 = 36.0;
/// 「添加音轨」行高（图标 18 + 行内边距 12 + 按钮内边距 10）。
pub const TRACK_ADD_ROW_H: f32 = 40.0;
/// 音轨行按钮水平内边距（旧版 `button::DEFAULT_PADDING` 左右各 10）。
pub const TRACK_BUTTON_PAD_X: f32 = 10.0;
/// 音轨行内容内边距（`row!().padding(4)`）。
pub const TRACK_ROW_PAD: f32 = 4.0;
/// 「添加音轨」行内容内边距（`row!().padding(6)`）。
pub const TRACK_ADD_ROW_PAD: f32 = 6.0;
/// 音轨编号 / 指挥轨图标列宽度（含 2px 左内边距）。
pub const TRACK_LABEL_COL_W: f32 = 36.0;
/// 「添加音轨」图标列宽度。
pub const TRACK_ADD_ICON_COL_W: f32 = 24.0;
/// 音轨行内的列间距。
pub const TRACK_COL_GAP: f32 = 4.0;
/// 音轨列表首行相对面板顶部的偏移（标题行 24 = 面板内边距 8 + 标题 16）。
pub const TRACK_LIST_HEADER_H: f32 = 24.0;
