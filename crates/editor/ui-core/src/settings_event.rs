//! Settings 事件子模块
//!
//! 设置面板的事件枚举（仅枚举，不包含面板逻辑）。

use lumino_core::storage::config::{
    AudioEngineKind, EraserBehavior, SelectionBoxMode, SynthBackend, TrackAddBehavior,
};
use lumino_extras::i18n::Language;

/// MIDI 输出类型（顶层选择）
///
/// 内置合成器（XSynth / LGS 等软件合成器）归为 `Builtin` 一类，其下再用
/// `SynthBackend` 选择具体引擎；KDMAPI / 系统 MIDI 为独立类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputType {
    /// 内置软件合成器
    Builtin,
    /// KDMAPI 系统驱动
    Kdmapi,
    /// 系统 MIDI (WinMM)
    System,
}

/// 设置面板事件
#[derive(Debug, Clone)]
pub enum Event {
    /// 选择设置面板菜单项（菜单索引）
    MenuSelected(usize),
    /// 合成器后端变更
    SynthBackendChanged(SynthBackend),
    /// MIDI 输出类型变更（内置合成器 / KDMAPI / 系统 MIDI）
    OutputTypeChanged(OutputType),
    /// 音频引擎后端变更（当前仅 Realtime）
    AudioEngineChanged(AudioEngineKind),
    /// 音色库路径变更
    SoundfontPathChanged(String),
    /// 请求浏览选择音色库文件
    BrowseSoundfont,
    /// 是否使用系统标题栏
    NativeTitlebarChanged(bool),
    /// XSynth 渲染缓冲区大小变更（毫秒）
    XSynthBufferChanged(f64),
    /// XSynth 采样率变更
    XSynthSampleRateChanged(u32),
    /// XSynth 释放音符时是否淡出
    XSynthFadeOutChanged(bool),
    /// XSynth 每键最大同音数变更（None 为不限）
    XSynthMaxVoicesChanged(Option<usize>),
    /// XSynth 每键最大同音数自定义输入变更
    XSynthMaxVoicesCustomInput(String),
    /// LGS (GPU) 缓冲区大小（GPU 块大小，2 的幂）变更
    LgsBlockSizeChanged(usize),
    /// LGS (GPU) 每键最大同音数变更（0=不限制）
    LgsMaxVoicesChanged(usize),
    /// LGS (GPU) 专属响度(力度)过滤阈值变更（0=关闭过滤，1-127）
    LgsVelocityFilterChanged(u8),
    /// 主题变更
    ThemeChanged(String),
    /// 橡皮擦工具行为变更
    EraserBehaviorChanged(EraserBehavior),
    /// 框选框显示模式变更
    SelectionBoxModeChanged(SelectionBoxMode),
    /// 程序字体名称变更
    ProgramFontNameChanged(String),
    /// 程序字体路径变更
    ProgramFontPathChanged(String),
    /// 请求浏览选择程序字体文件
    BrowseProgramFont,
    /// 自动滚动固定指示线位置变更
    AutoScrollFixedPositionChanged(String),
    /// 自动滚动翻页触发偏移变更
    AutoScrollPageTriggerOffsetChanged(String),
    /// 自动滚动翻页后指示线返回位置变更
    AutoScrollPageReturnPositionChanged(String),
    /// 力度过滤阈值变更（0-127）
    VelocityFilterThresholdChanged(String),
    /// 是否启用 HiDPI 图标渲染
    IconHiDPIChanged(bool),
    /// 是否启用 256 键扩展钢琴卷帘
    Enable256keyChanged(bool),
    /// 力度面板显示样式变更（曲线/柱状）
    VelocityCurveStyleChanged(bool),
    /// 选中 MIDI 设备（设备序号）
    DeviceSelected(u32),
    /// 界面语言变更
    LanguageChanged(Language),
    /// 高精度洋葱皮贴图是否启用
    HiresOnionEnabledChanged(bool),
    /// 高精度洋葱皮贴图每组小节数变更
    HiresMeasuresPerGroupChanged(String),
    /// 高精度洋葱皮贴图宽度变更
    HiresTileWidthChanged(String),
    /// 高精度洋葱皮贴图编辑后重生成冷静期变更
    HiresCooldownChanged(String),
    /// 高精度洋葱皮贴图 GPU 显存上限变更（MB）
    HiresGpuMemLimitChanged(String),
    /// 播放时键盘颜色指示是否启用
    PlaybackKeyColorsEnabledChanged(bool),
    /// 添加音轨行为变更
    TrackAddBehaviorChanged(TrackAddBehavior),
    /// 调色板名称变更
    PaletteChanged(String),
    // 编辑设置
    /// 操作日志总条数上限变更
    HistoryTotalLimitChanged(String),
    /// 单条日志条目上限变更
    HistoryEntryLimitChanged(String),
    /// 编辑历史合并窗口毫秒数变更
    MergeWindowMsChanged(String),
    /// 编辑拦截时是否显示 Toast 提示
    InterceptNotificationChanged(bool),
    /// 自动化曲线连线粗细（像素，1-10）
    AutomationLineThicknessChanged(f32),
    /// Tempo 面板 BPM 绘制上限（预设值下拉选择，如 256/512/.../65536）
    TempoMaxBpmChanged(f64),
    /// 请求打开"自定义 BPM 上限"输入面板
    TempoMaxBpmCustomOpen,
    /// 请求关闭"自定义 BPM 上限"输入面板
    TempoMaxBpmCustomClose,
    /// 自定义 BPM 上限输入框内容变化
    TempoMaxBpmCustomInput(String),
    /// 确认自定义 BPM 上限
    TempoMaxBpmCustomConfirm,
    /// 日志文件保留份数
    LogRetentionCountChanged(String),
    /// 底边栏监控数据刷新间隔（毫秒，50-2000）
    MonitorRefreshIntervalChanged(f32),
}
