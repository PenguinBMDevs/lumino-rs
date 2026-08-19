//! 全局常量定义
//!
//! 从 `lumino-ui-constants` crate 合并而来。
//! 渲染相关的颜色常量已在 `lumino-gfx::constants` 中定义，此处不重复。

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
        /// Y轴最小缩放（取消下限，避免高DPI下窗口物理像素大但缩放受限）
        pub const MIN_ZOOM_Y: f32 = 0.5;
        /// Y轴最大缩放
        pub const MAX_ZOOM_Y: f32 = 100.0;

        /// 走带视图 X 轴最小缩放
        pub const MIN_ARRANGEMENT_ZOOM_X: f32 = 0.01;
        /// 走带视图 X 轴最大缩放
        pub const MAX_ARRANGEMENT_ZOOM_X: f32 = 10.0;
        /// 走带视图 Y 轴最小缩放
        pub const MIN_ARRANGEMENT_ZOOM_Y: f32 = 0.2;
        /// 走带视图 Y 轴最大缩放
        pub const MAX_ARRANGEMENT_ZOOM_Y: f32 = 5.0;
    }

    /// 默认最小 tick 值（用于画布宽度下限）
    pub const DEFAULT_MIN_TICKS: f32 = 960.0 * 4.0;

    /// 弯音调制范围（-8192 ~ 8191）
    pub const PITCH_BEND_RANGE: i16 = 8191;
    /// 弯音调制转换因子
    pub const PITCH_BEND_FACTOR: f32 = PITCH_BEND_RANGE as f32;

    /// 可见琴键数量限制
    pub const MIN_VISIBLE_KEY_COUNT: u16 = 1;
    /// 最大可见琴键数量
    pub const MAX_VISIBLE_KEY_COUNT: u16 = 256;

    /// 双击检测时间阈值（毫秒）
    pub const DOUBLE_CLICK_TIME_MS: u128 = 300;

    /// 双击检测距离阈值（像素）
    pub const DOUBLE_CLICK_DISTANCE_PX: f32 = 10.0;

    /// 远程光标圆圈半径（像素）
    pub const REMOTE_CURSOR_CIRCLE_RADIUS: f32 = 8.0;

    /// 光标标签字体大小
    pub const CURSOR_LABEL_FONT_SIZE: f32 = 11.0;

    /// 远程光标边框宽度
    pub const REMOTE_CURSOR_BORDER_WIDTH: f32 = 1.0;

    /// 用户名片内边距
    pub const USERNAME_LABEL_PADDING: f32 = 4.0;

    /// 用户名片高度
    pub const USERNAME_LABEL_HEIGHT: f32 = 18.0;

    /// 用户名片圆角半径
    pub const USERNAME_LABEL_BORDER_RADIUS: f32 = 4.0;

    /// 用户名片箭头偏移
    pub const USERNAME_LABEL_ARROW_OFFSET: f32 = 4.0;

    /// 滚动最大增量
    pub const SCROLL_MAX_DELTA: f32 = 100.0;

    /// 滚动线条缩放系数
    pub const SCROLL_LINES_SCALE: f32 = 30.0;

    /// 小节线宽度
    pub const BAR_LINE_WIDTH: f32 = 4.0;

    /// 拍线宽度
    pub const BEAT_LINE_WIDTH: f32 = 1.0;

    /// 半拍线宽度
    pub const HALF_BEAT_LINE_WIDTH: f32 = 0.5;

    /// 网格线宽度
    pub const GRID_LINE_WIDTH: f32 = 0.5;

    /// 网格线透明度
    pub const GRID_LINE_ALPHA: f32 = 0.1;

    /// 选择框最小尺寸
    pub const SELECTION_BOX_MIN_SIZE: f32 = 1.0;

    /// 框选框统一边框宽度（像素）
    pub const SELECTION_BOX_STROKE_WIDTH: f32 = 3.0;

    /// 框选框统一边框颜色（灰色）
    pub const SELECTION_BOX_STROKE_COLOR: iced_core::Color =
        iced_core::Color::from_rgb(0.62, 0.62, 0.62);

    /// 框选框统一填充颜色（比边框颜色浅一点的灰色 + 半透明）
    pub const SELECTION_BOX_FILL_COLOR: iced_core::Color =
        iced_core::Color::from_rgba(0.78, 0.78, 0.78, 0.35);

    /// 时间轴标尺高度（小节号显示区域）
    pub const RULER_HEIGHT: f32 = 24.0;

    /// 小节号字体大小
    pub const MEASURE_NUMBER_FONT_SIZE: f32 = 12.5;

    /// 钢琴键音高标签字体大小
    pub const KEY_LABEL_FONT_SIZE: f32 = 11.0;

    /// 演奏指示线宽度
    pub const PLAYBACK_INDICATOR_WIDTH: f32 = 2.0;

    /// 演奏指示线顶部三角形大小
    pub const PLAYBACK_INDICATOR_TRIANGLE_SIZE: f32 = 8.0;

    /// 剪贴板格式标识
    pub const CLIPBOARD_FORMAT: &str = "notes";
    /// 剪贴板格式版本号
    pub const CLIPBOARD_VERSION: u32 = 1;

    /// 默认音符力度
    pub const DEFAULT_NOTE_VELOCITY: u8 = 100;
    /// 默认 MIDI 通道
    pub const DEFAULT_MIDI_CHANNEL: u8 = 0;
    /// 默认粘贴锚点音高（中央 C）
    pub const DEFAULT_PASTE_ANCHOR_KEY: u16 = 60;
}

/// 内存管理常量
pub mod memory {
    /// 默认内存限制（1GB）
    pub const DEFAULT_MEMORY_LIMIT_BYTES: usize = 1024 * 1024 * 1024;

    /// 默认整合组贴图内存限制（MB）
    pub const DEFAULT_GROUP_TILE_MEM_LIMIT_MB: u32 = 256;
}

/// 时序/物理常量
pub mod timing {
    /// 默认帧间隔（秒，~60fps）— 用于弹簧动画首次 update 的 dt fallback
    pub const DEFAULT_FRAME_TIME_SECS: f64 = 0.016;

    /// 默认高精度贴图重生成冷却时间（秒）
    pub const DEFAULT_HIRES_COOLDOWN_SECS: u64 = 10;
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

/// 滚动条相关常量
pub mod scrollbar {
    /// 边缘检测宽度（像素）
    pub const EDGE_WIDTH_PX: f32 = 6.0;

    /// 滑块最小尺寸（像素）
    pub const THUMB_MIN_SIZE_PX: f32 = 20.0;

    /// 内边距（像素）
    pub const PADDING_PX: f32 = 4.0;

    /// 轨道尺寸（像素）
    pub const TRACK_SIZE_PX: f32 = 20.0;

    /// 轨道内边距
    pub const TRACK_PADDING: f32 = 2.0;

    /// 轨道与滑块之间的间距
    pub const TRACK_THUMB_GAP: f32 = 2.0;

    /// 滑块与轨道边缘的间距
    pub const THUMB_TRACK_EDGE_GAP: f32 = 2.0;
}

/// 协作相关常量
pub mod collaboration {
    /// 默认协作服务器端口
    pub const DEFAULT_PORT: u16 = 3000;
}
