//! 标题栏 / 菜单 / 状态栏的只读模型。

/// 自定义标题栏状态。
#[derive(Debug, Clone, Default)]
pub struct TitlebarModel {
    /// 窗口是否聚焦（决定标题栏底色：`background.neutral` / `background.weaker`）。
    pub focused: bool,
    /// 菜单标签（文件 / 编辑 / 视图 / 帮助，按当前语言）。
    pub menus: Vec<String>,
    /// 可交互菜单树（`None` = 只画标签，交互仍交给 iced）。
    pub menu: Option<MenuBarModel>,
    /// 系统原生标题栏模式：只画 30px 菜单栏（无 logo、窗口控制、拖动区）。
    pub menu_only: bool,
}

/// 标题栏菜单栏（生产 `titlebar::menu` 的数据镜像）。
#[derive(Debug, Clone, Default)]
pub struct MenuBarModel {
    /// 四个主菜单（文件 / 编辑 / 视图 / 帮助）。
    pub menus: Vec<MenuModel>,
}

/// 单个主菜单。
#[derive(Debug, Clone, Default)]
pub struct MenuModel {
    /// 菜单标签。
    pub label: String,
    /// 菜单项。
    pub entries: Vec<MenuEntry>,
}

/// 菜单项（与生产 `titlebar::menu::MenuItem` 一一对应）。
#[derive(Debug, Clone)]
pub enum MenuEntry {
    /// 可点击动作；`action` 是在整棵菜单树里的遍历序号。
    Action {
        /// 本地化显示名。
        label: String,
        /// 是否可点击（生产 `ActionDisabled` 为 `false`）。
        enabled: bool,
        /// 动作序号。
        action: usize,
    },
    /// 分隔线。
    Separator,
    /// 子菜单（当前生产只有一级）。
    Submenu {
        /// 本地化显示名。
        label: String,
        /// 子项。
        entries: Vec<MenuEntry>,
    },
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
