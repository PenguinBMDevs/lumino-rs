//! 生产状态 → egui 外壳的只读视图模型。
//!
//! `lumino-ui-egui` 不能依赖 `lumino-ui`（会引入 iced），因此由生产侧
//! （`Root::egui_chrome_model`）把状态填进这里的纯数据类型，egui 只负责绘制。
//! 这样同一份渲染代码既能被离线场景复用，也能被真实窗口叠加层复用。

use lumino_core::Tool;
use lumino_core::storage::config::AutoScrollMode;

/// 主窗口外壳的只读快照。
#[derive(Debug, Clone, Default)]
pub struct ChromeModel {
    /// 标题栏；`None` = 使用系统原生标题栏，egui 不接管。
    pub titlebar: Option<TitlebarModel>,
    /// 工具栏；`None` = 不接管。
    pub toolbar: Option<ToolbarModel>,
    /// 状态栏。
    pub statusbar: StatusbarModel,
}

/// 工具栏（72px band）的只读状态。
#[derive(Debug, Clone)]
pub struct ToolbarModel {
    /// 工具栏左边界（左侧栏占用宽度，逻辑点）。
    pub left_inset: f32,
    /// BPM 文本（保留 1 位小数）。
    pub bpm: String,
    /// PPQ 文本（形如 `PPQ 1920`）。
    pub ppq: String,
    /// 播放位置「小节.拍」。
    pub position: String,
    /// 播放时间 `mm:ss.cs`。
    pub time: String,
    /// 吸附规格文本（形如 `1/4`）。
    pub snap: String,
    /// 当前工具。
    pub tool: Tool,
    /// 绘制工具面板是否打开（第 5 个选择器的选中态）。
    pub draw_panel_open: bool,
    /// 是否正在播放（播放按钮显示暂停图标）。
    pub playing: bool,
    /// 是否循环播放（循环按钮图标切换）。
    pub looping: bool,
    /// 是否正在录制。
    pub recording: bool,
    /// 洋葱皮开关（`background.strong` 底色表示开启）。
    pub onion: bool,
    /// 自动滚动模式（决定右侧图标）。
    pub auto_scroll: AutoScrollMode,
}

impl Default for ToolbarModel {
    fn default() -> Self {
        Self {
            left_inset: 0.0,
            bpm: "120.0".to_owned(),
            ppq: "PPQ 1920".to_owned(),
            position: "1.1".to_owned(),
            time: "00:00.00".to_owned(),
            snap: "1/4".to_owned(),
            tool: Tool::Pointer,
            draw_panel_open: false,
            playing: false,
            looping: false,
            recording: false,
            onion: false,
            auto_scroll: AutoScrollMode::ScrollingIndicator,
        }
    }
}

/// 自定义标题栏状态。
#[derive(Debug, Clone, Default)]
pub struct TitlebarModel {
    /// 窗口是否聚焦（决定标题栏底色：`background.neutral` / `background.weaker`）。
    pub focused: bool,
    /// 菜单标签（文件 / 编辑 / 视图 / 帮助，按当前语言）。
    pub menus: Vec<String>,
}

/// 状态栏状态。
#[derive(Debug, Clone, Default)]
pub struct StatusbarModel {
    /// 左侧描述文本（悬停提示 / 就绪 / 音符数）。
    pub left: String,
    /// 左侧文本是否用强调色（悬停说明 / FPS 风格）。
    pub left_accent: bool,
    /// CPU 占用百分比；`None` = 尚未采样，整组指标隐藏。
    pub cpu_percent: Option<f32>,
    /// 进程内存（MB）。
    pub memory_mb: f32,
    /// 平滑后的 FPS。
    pub fps: Option<f32>,
    /// 右侧补充文本（生产 `StatusInfo.right_text`）。
    pub right: String,
}

impl StatusbarModel {
    /// 是否显示 CPU / MEM / FPS 指标组（生产：`fps.is_some()`）。
    pub fn has_metrics(&self) -> bool {
        self.fps.is_some()
    }
}
