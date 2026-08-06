//! 右侧栏核心数据结构与常量

/// 右侧栏图标栏宽度（固定，与左侧栏路由栏一致）
pub const ROUTE_BAR_WIDTH: f32 = 48.0;
/// 右侧栏面板默认宽度（与左侧栏面板一致）
pub const DEFAULT_PANEL_WIDTH: f32 = 200.0;
/// 右侧栏面板最小宽度
pub const MIN_PANEL_WIDTH: f32 = 150.0;
/// 右侧栏面板最大宽度
pub const MAX_PANEL_WIDTH: f32 = 900.0;
/// 右侧栏调整大小手柄宽度
pub const RESIZE_HANDLE_WIDTH: f32 = 6.0;

/// 右侧栏状态
#[derive(Debug, Clone)]
pub struct RightSidebar {
    /// 面板是否可见
    pub panel_visible: bool,
    /// 面板宽度
    pub panel_width: f32,
    /// 是否正在拖拽调整宽度
    pub is_resizing: bool,
    /// 拖拽开始时的鼠标 X 坐标
    pub resize_start_x: f32,
    /// 拖拽开始时的面板宽度
    pub resize_start_width: f32,
    /// 用户通过文件对话框选中的待转换图片路径
    pub selected_image_path: Option<std::path::PathBuf>,
}

impl RightSidebar {
    pub fn new() -> Self {
        Self {
            panel_visible: false,
            panel_width: DEFAULT_PANEL_WIDTH,
            is_resizing: false,
            resize_start_x: 0.0,
            resize_start_width: DEFAULT_PANEL_WIDTH,
            selected_image_path: None,
        }
    }

    /// 计算右侧栏总宽度（图标栏 + 面板）
    pub fn width(&self) -> u32 {
        (ROUTE_BAR_WIDTH
            + if self.panel_visible {
                self.panel_width
            } else {
                0.0
            }) as u32
    }

    /// 切换面板显示/隐藏
    pub fn toggle_panel(&mut self) {
        self.panel_visible = !self.panel_visible;
    }

    /// 设置选中的图片路径（并确保面板展开以便查看结果）
    pub fn set_selected_image_path(&mut self, path: std::path::PathBuf) {
        self.selected_image_path = Some(path);
        self.panel_visible = true;
    }

    /// 开始拖拽调整面板宽度
    pub fn start_resize(&mut self, cursor_x: f32) {
        self.is_resizing = true;
        self.resize_start_x = cursor_x;
        self.resize_start_width = self.panel_width;
    }

    /// 更新拖拽位置
    pub fn update_resize_position(&mut self, cursor_x: f32) {
        if self.is_resizing {
            // 右侧栏的拖拽方向与左侧相反：鼠标左移增大面板
            let delta_x = self.resize_start_x - cursor_x;
            let new_width = self.resize_start_width + delta_x;
            self.panel_width = new_width.clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        }
    }

    /// 结束拖拽
    pub fn end_resize(&mut self) {
        self.is_resizing = false;
    }
}

impl Default for RightSidebar {
    fn default() -> Self {
        Self::new()
    }
}
