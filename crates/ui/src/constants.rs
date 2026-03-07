//! 全局常量定义

/// UI 尺寸相关常量
pub mod dimensions {
    /// 窗口最小宽度
    pub const MIN_WINDOW_WIDTH: u32 = 800;
    /// 窗口最小高度
    pub const MIN_WINDOW_HEIGHT: u32 = 600;

    /// 菜单安全区域（顶部留白，防止与下拉菜单重叠）
    pub const MENU_SAFE_ZONE: f32 = 40.0;

    /// 滚动条边缘宽度
    pub const SCROLLBAR_EDGE_WIDTH: f32 = 5.0;

    /// 默认图标尺寸
    pub const DEFAULT_ICON_SIZE: u32 = 24;
    /// 窗口控制图标尺寸
    pub const WINDOW_ICON_SIZE: u32 = 20;
}

/// 编辑器相关常量
pub mod editor {
    /// 音符边缘检测阈值（像素）
    pub const NOTE_EDGE_THRESHOLD_PX: f32 = 10.0;

    /// 拖动启动阈值（按键高度的比例）
    pub const DRAG_START_THRESHOLD_RATIO: f32 = 0.5;

    /// 预览音符透明度
    pub const PREVIEW_NOTE_OPACITY: f32 = 0.5;

    /// 缩放限制
    pub mod zoom {
        /// X轴最小缩放
        pub const MIN_ZOOM_X: f32 = 0.001;
        /// X轴最大缩放
        pub const MAX_ZOOM_X: f32 = 10.0;
        /// Y轴最小缩放
        pub const MIN_ZOOM_Y: f32 = 5.0;
        /// Y轴最大缩放
        pub const MAX_ZOOM_Y: f32 = 100.0;
    }

    /// 可见琴键数量限制
    pub const MIN_VISIBLE_KEY_COUNT: u16 = 1;
    pub const MAX_VISIBLE_KEY_COUNT: u16 = 256;
}

/// 渲染相关常量
pub mod rendering {
    /// 初始缓冲区容量
    pub const INITIAL_INSTANCE_CAPACITY: usize = 1024;

    /// 主题亮度阈值（用于判断暗色/亮色主题）
    pub const THEME_BRIGHTNESS_THRESHOLD: f32 = 0.5;

    /// 网格线宽度
    pub const GRID_LINE_WIDTH: f32 = 0.5;

    /// 网格点击时间阈值（毫秒）
    pub const GRID_CLICK_TIME_THRESHOLD_MS: u128 = 300;
    /// 网格点击位置阈值（像素）
    pub const GRID_CLICK_POS_THRESHOLD_PX: f32 = 10.0;
    /// 网格滚动最大增量
    pub const GRID_SCROLL_MAX_DELTA: f32 = 100.0;
}

/// 内存管理常量
pub mod memory {
    /// 默认内存限制（1GB）
    pub const DEFAULT_MEMORY_LIMIT_BYTES: usize = 1024 * 1024 * 1024;
}

/// 进度相关常量
pub mod progress {
    /// 初始解析进度
    pub const PARSE_PROGRESS_START: f32 = 100.0;
    /// 中间进度点
    pub const PROGRESS_MIDWAY: f32 = 0.5;
}

/// 布局间距
pub mod spacing {
    /// 默认内容间距
    pub const DEFAULT: f32 = 10.0;
    /// 图标标签间距
    pub const ICON_LABEL: f32 = 8.0;
    /// 主间距
    pub const MAIN: f32 = 16.0;
}

/// 洋葱皮默认颜色
pub mod onion_skin {
    use iced_core::Color;

    /// 洋葱皮默认透明度
    pub const DEFAULT_OPACITY: f32 = 0.3;

    /// 默认颜色列表
    pub const DEFAULT_COLORS: [Color; 6] = [
        Color::from_rgb(1.0, 0.5, 0.31),   // 橙色
        Color::from_rgb(0.53, 0.81, 0.92), // 天蓝色
        Color::from_rgb(0.56, 0.93, 0.56), // 浅绿色
        Color::from_rgb(0.93, 0.51, 0.93), // 紫色
        Color::from_rgb(0.5, 1.0, 0.0),    // 黄绿色
        Color::from_rgb(0.98, 0.5, 0.45),  // 珊瑚色
    ];
}
