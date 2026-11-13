//! 标题栏 / 菜单 / 状态栏的只读模型。

/// 自定义标题栏状态。
#[derive(Debug, Clone, Default)]
pub struct TitlebarModel {
    /// 窗口是否聚焦（决定标题栏底色：`background.neutral` / `background.weaker`）。
    pub focused: bool,
    /// 菜单标签（文件 / 编辑 / 视图 / 帮助，按当前语言）。
    pub menus: Vec<String>,
    /// 可交互菜单树（`None` = 只画标签，交互仍交给 旧版）。
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
    /// GPU (LGS) 指标组；`None` = 设置关闭或后端未就绪。
    pub gpu: Option<GpuMetricsModel>,
}

/// GPU 队列趋势（`statusbar::QueueTrend` 的数据镜像）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuQueueTrend {
    /// 上升（缓冲变深）。
    Up,
    /// 下降（缓冲变浅，趋向欠载）。
    Down,
    /// 持平。
    #[default]
    Flat,
}

impl GpuQueueTrend {
    /// 箭头显示字符（与 旧版 `QueueTrend::arrow` 逐字一致）。
    pub fn arrow(self) -> &'static str {
        match self {
            Self::Up => "↑",
            Self::Down => "↓",
            Self::Flat => "→",
        }
    }
}

/// 指标颜色档位（负载 / 欠载 / 治理级别共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricTone {
    /// 常规（强调色）。
    Normal,
    /// 警告（黄）。
    Warning,
    /// 危险（红）。
    Danger,
}

/// 负载分级：<60% 常规；60–90% 警告；≥90% 危险（1.0 = 90% 实时预算上限）。
pub fn load_tone(load: f64) -> MetricTone {
    if load >= 0.9 {
        MetricTone::Danger
    } else if load >= 0.6 {
        MetricTone::Warning
    } else {
        MetricTone::Normal
    }
}

/// 欠载分级：0 = 常规；>0 = 危险（欠载即已静音丢音）。
pub fn underrun_tone(underruns: u64) -> MetricTone {
    if underruns == 0 {
        MetricTone::Normal
    } else {
        MetricTone::Danger
    }
}

/// 治理级别分级：0=常规 / 1=警告 / ≥2=危险。
pub fn governor_tone(level: u32) -> MetricTone {
    match level {
        0 => MetricTone::Normal,
        1 => MetricTone::Warning,
        _ => MetricTone::Danger,
    }
}

/// 状态栏 GPU (LGS) 指标组（`statusbar::metrics::build_gpu_section` 的数据镜像）。
#[derive(Debug, Clone, PartialEq)]
pub struct GpuMetricsModel {
    /// 队列深度（毫秒）。
    pub queue_ms: f64,
    /// 队列趋势。
    pub trend: GpuQueueTrend,
    /// 渲染负载（0..≈1+；`GpuLoadDisplay` 决定取瞬时还是短窗均值）。
    pub load: f64,
    /// 活跃声部数。
    pub voice_count: u64,
    /// 累计欠载次数。
    pub underruns: u64,
    /// 采样显存文本（`None` = 未加载音源，不占位）。
    pub sample_gpu: Option<String>,
    /// 采样内存文本（`None` = 未加载音源，不占位）。
    pub sample_cpu: Option<String>,
    /// 防爆治理级别（`None` = 未开启显示或级别为 0）。
    pub governor_level: Option<u32>,
    /// 被丢弃的过期 NoteOn 数（`None` = 未开启显示或为 0）。
    pub dropped: Option<u64>,
}

impl StatusbarModel {
    /// 是否显示 CPU / MEM / FPS 指标组（生产：`fps.is_some()`）。
    pub fn has_metrics(&self) -> bool {
        self.fps.is_some()
    }
}
