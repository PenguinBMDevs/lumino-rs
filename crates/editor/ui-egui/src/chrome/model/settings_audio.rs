//! 设置「音频」页的只读模型。
//!
//! 页面随当前合成器后端动态变化（XSynth / LGS / 系统 MIDI），所有行统一用
//! `AudioRowModel` 描述：标签 + 控件 + 控件宽度 + 字段标识；文字全部由生产侧
//! 按当前语言预计算。点击/拖动只回传字段级动作，生产再按自身数据结构解析。

/// 音频页。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioPageModel {
    /// 标题。
    pub title: String,
    /// 页面行（自上而下）。
    pub rows: Vec<AudioRowModel>,
}

/// 一行控件。
#[derive(Debug, Clone, PartialEq)]
pub struct AudioRowModel {
    /// 字段标识（动作回传时原样带回）。
    pub field: AudioField,
    /// 标签文本。
    pub label: String,
    /// 悬浮说明（空串 = 不挂 tooltip）。
    pub hint: String,
    /// 控件。
    pub control: AudioControlModel,
    /// 控件宽度（逻辑点）。
    pub width: f32,
}

/// 音频页可交互字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioField {
    /// 顶层输出类型（内置合成器 / KDMAPI / 系统 MIDI）。
    OutputType,
    /// 内置合成器引擎（XSynth / LGS）。
    Engine,
    /// 音频播放输出设备。
    AudioOutput,
    /// 刷新音频输出设备（按钮）。
    ScanAudioOutputs,
    /// MIDI 输入设备。
    MidiDevice,
    /// WinMM 输出设备（播表）。
    WinmmOutput,
    /// 刷新 WinMM 输出（按钮）。
    ScanWinmmOutputs,
    /// 音色库路径（输入框）。
    SoundfontPath,
    /// 浏览音色库（按钮）。
    BrowseSoundfont,
    /// XSynth 缓冲（毫秒）。
    XSynthBuffer,
    /// XSynth 每键最大同音数（滑杆）。
    XSynthMaxVoices,
    /// XSynth 每键最大同音数（自定义输入）。
    XSynthMaxVoicesCustom,
    /// XSynth 全局复音上限。
    XSynthGlobalLimit,
    /// XSynth 复音软目标比例。
    XSynthRatio,
    /// XSynth 过载保命闸。
    XSynthGate,
    /// XSynth 力度过滤。
    XSynthVelocity,
    /// LGS 块大小（2 的指数）。
    LgsBlock,
    /// LGS 离线导出块长。
    LgsOfflineBlock,
    /// LGS 每键最大同音数。
    LgsMaxVoices,
    /// LGS 全局复音上限。
    LgsGlobalLimit,
    /// LGS 过载保命闸。
    LgsGate,
    /// LGS 响度过滤。
    LgsVelocity,
    /// 状态栏 GPU 缓冲状态指示器。
    LgsGpuBufferStatus,
    /// GOV/DROP 治理指标段。
    LgsGpuGovernorMetrics,
    /// LOAD 显示口径。
    LgsGpuLoadDisplay,
}

/// 控件种类。
#[derive(Debug, Clone, PartialEq)]
pub enum AudioControlModel {
    /// 下拉框（显示文案 + 选中下标）。
    Combo {
        /// 选项文本。
        options: Vec<String>,
        /// 当前下标。
        selected: usize,
    },
    /// 滑杆（隐藏自带数值，数值由标签显示）。
    Slider {
        /// 当前值。
        value: f32,
        /// 下限。
        min: f32,
        /// 上限。
        max: f32,
        /// 步进。
        step: f64,
    },
    /// 文本输入框。
    Text {
        /// 当前文本。
        value: String,
        /// 占位符（空串 = 无）。
        placeholder: String,
    },
    /// 对钩框（标签在右侧同一行）。
    Checkbox {
        /// 是否勾选。
        checked: bool,
    },
    /// 按钮（标签即按钮文字）。
    Button,
    /// 只读文本（灰字；无设备 / 锁定提示）。
    Static {
        /// 文本。
        text: String,
        /// 是否辅助色。
        dim: bool,
    },
}
