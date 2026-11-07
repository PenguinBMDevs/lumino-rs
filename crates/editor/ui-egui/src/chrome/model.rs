//! 生产状态 → egui 外壳的只读视图模型。
//!
//! `lumino-ui-egui` 不能依赖 `lumino-ui`（会引入 iced），因此由生产侧
//! （`Root::egui_chrome_model`）把状态填进这里的纯数据类型，egui 只负责绘制。
//! 这样同一份渲染代码既能被离线场景复用，也能被真实窗口叠加层复用。

/// 主窗口外壳的只读快照。
#[derive(Debug, Clone, Default)]
pub struct ChromeModel {
    /// 标题栏；`None` = 使用系统原生标题栏，egui 不接管。
    pub titlebar: Option<TitlebarModel>,
    /// 状态栏。
    pub statusbar: StatusbarModel,
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
