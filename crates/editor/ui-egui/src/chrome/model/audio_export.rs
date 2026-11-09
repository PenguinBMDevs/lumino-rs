//! 音频导出对话框模型（全部文案与选项由生产侧预计算）。

/// 音频导出面板（含占用的区域矩形）。
#[derive(Debug, Clone)]
pub struct AudioExportPanelModel {
    /// 面板占据的窗口区域（逻辑坐标；含工具栏 / 状态栏，生产整块替换右侧内容）。
    pub rect: egui::Rect,
    /// 面板内容。
    pub content: AudioExportModel,
}

impl Default for AudioExportPanelModel {
    fn default() -> Self {
        Self {
            rect: egui::Rect::NOTHING,
            content: AudioExportModel::default(),
        }
    }
}

/// 音频导出面板：标题 / 工程信息 / 音频设置 / 事件过滤 / 输出路径 / 按钮。
#[derive(Debug, Clone, Default)]
pub struct AudioExportModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 大标题「音频导出」（18px）。
    pub title: String,
    /// 工程信息（名称 / MIDI / 音色库）。
    pub project: AudioProjectModel,
    /// 音频设置区块。
    pub settings: AudioSettingsModel,
    /// 事件过滤区块。
    pub filter: AudioFilterModel,
    /// 输出路径区块。
    pub output: AudioOutputModel,
    /// 底部按钮区（按渲染状态三态）。
    pub buttons: AudioButtonsModel,
}

/// 工程信息区块。
#[derive(Debug, Clone, Default)]
pub struct AudioProjectModel {
    /// 「工程名称」标签。
    pub name_label: String,
    /// 工程名称输入值。
    pub name: String,
    /// 占位（与标签同文案）。
    pub name_hint: String,
    /// 「MIDI 路径」标签。
    pub midi_label: String,
    /// 已选 MIDI 路径（只读展示）。
    pub midi_path: String,
    /// 「音色库 (SF2)」标签。
    pub soundfont_label: String,
    /// 已选音色库路径（只读展示）。
    pub soundfont_path: String,
    /// 「浏览...」按钮。
    pub browse: String,
}

/// 一个下拉选项（`index` 是固定选项表里的下标，动作按此回传）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioChoice {
    /// 固定选项表下标。
    pub index: usize,
    /// 显示名。
    pub label: String,
    /// 是否为当前选择。
    pub selected: bool,
}

/// 复选框标识（动作按此映射回生产字段）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AudioToggleId {
    /// 启用限幅器。
    #[default]
    ApplyLimiter,
    /// 禁用淡出。
    DisableFadeOut,
    /// 线性包络。
    LinearEnvelope,
    /// 忽略音色变化。
    IgnoreProgram,
    /// 启用力度过滤。
    FilterVelocity,
    /// 启用键位过滤。
    FilterKey,
}

/// 一个复选框。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioToggle {
    /// 字段标识。
    pub id: AudioToggleId,
    /// 标签（长说明已并入生产文案）。
    pub label: String,
    /// 是否勾选。
    pub checked: bool,
    /// 是否可交互（GPU 后端下限幅 / 淡出固定，禁用）。
    pub enabled: bool,
}

/// 音频设置区块。
#[derive(Debug, Clone, Default)]
pub struct AudioSettingsModel {
    /// 区块标题「音频设置」。
    pub title: String,
    /// 渲染后端。
    pub backend_label: String,
    /// 后端选项（CPU / GPU）。
    pub backends: Vec<AudioChoice>,
    /// 后端说明。
    pub backend_hint: String,
    /// 输出格式。
    pub format_label: String,
    /// 格式选项。
    pub formats: Vec<AudioChoice>,
    /// 格式说明。
    pub format_hint: String,
    /// 比特率。
    pub bitrate_label: String,
    /// 比特率当前值（文本）。
    pub bitrate: String,
    /// 比特率说明。
    pub bitrate_hint: String,
    /// 采样率。
    pub sample_rate_label: String,
    /// 采样率选项。
    pub sample_rates: Vec<AudioChoice>,
    /// 采样率说明。
    pub sample_rate_hint: String,
    /// 通道数。
    pub channels_label: String,
    /// 通道选项。
    pub channels: Vec<AudioChoice>,
    /// 每键复音。
    pub layers_label: String,
    /// 每键复音当前值（文本）。
    pub layers: String,
    /// 每键复音说明。
    pub layers_hint: String,
    /// `layers == 0` 时的危险色警告行。
    pub layers_warning: Option<String>,
    /// 通道多线程。
    pub channel_thread_label: String,
    /// 通道多线程选项。
    pub channel_threads: Vec<AudioChoice>,
    /// 按键多线程。
    pub key_thread_label: String,
    /// 按键多线程选项。
    pub key_threads: Vec<AudioChoice>,
    /// 插值算法。
    pub interpolation_label: String,
    /// 插值选项。
    pub interpolations: Vec<AudioChoice>,
    /// 插值说明。
    pub interpolation_hint: String,
    /// 三个复选框（限幅 / 淡出 / 线性包络）。
    pub toggles: Vec<AudioToggle>,
}

/// 事件过滤区块。
#[derive(Debug, Clone, Default)]
pub struct AudioFilterModel {
    /// 区块标题「事件过滤」。
    pub title: String,
    /// 忽略音色变化复选框。
    pub ignore: AudioToggle,
    /// 忽略音色变化说明。
    pub ignore_hint: String,
    /// 力度过滤说明（标题行）。
    pub velocity_title: String,
    /// 力度过滤复选框。
    pub velocity_toggle: AudioToggle,
    /// 力度范围标签。
    pub velocity_range_label: String,
    /// 力度下限（文本）。
    pub velocity_low: String,
    /// 力度上限（文本）。
    pub velocity_high: String,
    /// 键位过滤说明（标题行）。
    pub key_title: String,
    /// 键位过滤复选框。
    pub key_toggle: AudioToggle,
    /// 键位范围标签。
    pub key_range_label: String,
    /// 键位下限（文本）。
    pub key_low: String,
    /// 键位上限（文本）。
    pub key_high: String,
    /// 音符结束延迟标签。
    pub delay_label: String,
    /// 音符结束延迟（文本，毫秒）。
    pub delay: String,
    /// 音符结束延迟说明。
    pub delay_hint: String,
}

/// 输出路径区块。
#[derive(Debug, Clone, Default)]
pub struct AudioOutputModel {
    /// 区块标题「输出路径」。
    pub title: String,
    /// 路径输入值。
    pub path: String,
    /// 占位（「选择输出路径...」）。
    pub path_hint: String,
    /// 「浏览...」按钮。
    pub browse: String,
}

/// 底部按钮区（三态）。
#[derive(Debug, Clone, Default)]
pub struct AudioButtonsModel {
    /// 当前状态。
    pub mode: AudioButtonsMode,
}

/// 按钮区状态。
#[derive(Debug, Clone)]
pub enum AudioButtonsMode {
    /// 未渲染：关闭 / 导出。
    Actions {
        /// 关闭按钮。
        close: String,
        /// 导出按钮。
        export: String,
    },
    /// 渲染中：状态行 + 进度条 + 百分比 + 暂停 / 中止。
    Rendering {
        /// 状态文案（已暂停 / 进度消息）。
        status: String,
        /// 进度 0..=1。
        progress: f32,
        /// 百分比文本。
        percentage: String,
        /// 暂停 / 继续按钮。
        pause: String,
        /// 中止按钮。
        abort: String,
    },
    /// 渲染完成 / 失败：状态行 + 重置 / 关闭。
    Finished {
        /// 状态文案。
        status: String,
        /// 状态语义（完成绿 / 失败红 / 中止普通）。
        kind: crate::chrome::ExportMessageKind,
        /// 重置按钮。
        reset: String,
        /// 关闭按钮。
        close: String,
    },
}

impl Default for AudioButtonsMode {
    fn default() -> Self {
        Self::Actions {
            close: String::new(),
            export: String::new(),
        }
    }
}
