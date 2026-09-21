use serde::{Deserialize, Serialize};

// 音轨标签格式同 yinhe：{通道字母}{通道号+1:02}，音轨始终按原始序号排列。
// 通道字母 ch0=A, ch1=B, ..., ch15=P，通道号 1-16（零填充两位数）。

/// 添加音轨时的行为
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackAddBehavior {
    /// 自动跳转到被添加的新音轨
    #[default]
    AutoSwitch,
    /// 保持当前音轨位置不变
    StayCurrent,
}

impl std::fmt::Display for TrackAddBehavior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackAddBehavior::AutoSwitch => write!(f, "自动跳转到新音轨"),
            TrackAddBehavior::StayCurrent => write!(f, "保持当前音轨"),
        }
    }
}

/// 合成器后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SynthBackend {
    /// 内置 XSynth 合成器（默认）
    #[default]
    XSynth,
    /// KDMAPI 合成器（调用系统 KDMAPI）
    Kdmapi,
    /// 系统 MIDI 合成器
    System,
    /// LGS (GPU) 合成器（基于 lumino-gpu-synth 的 GPU 加速渲染）
    Lgs,
}

/// 框选框显示模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelectionBoxMode {
    /// 弹簧动画模式：框选框边界有弹性动画效果
    Spring,
    /// 直接跟随模式：框选框直接跟随鼠标，无动画延迟
    #[default]
    Direct,
}

impl std::fmt::Display for SelectionBoxMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectionBoxMode::Spring => write!(f, "弹簧动画"),
            SelectionBoxMode::Direct => write!(f, "直接跟随"),
        }
    }
}

/// 橡皮擦工具行为模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EraserBehavior {
    /// 默认模式：Shift+拖动框选删除，普通点击删除单个
    #[default]
    Default,
    /// 直接框选模式：拖动框选删除，Shift+点击删除单个
    DirectSelect,
}

impl std::fmt::Display for EraserBehavior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EraserBehavior::Default => write!(f, "默认 (Shift+拖动框选)"),
            EraserBehavior::DirectSelect => write!(f, "直接框选 (无需Shift)"),
        }
    }
}

impl std::fmt::Display for SynthBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SynthBackend::XSynth => write!(f, "XSynth (内置)"),
            SynthBackend::Kdmapi => write!(f, "KDMAPI"),
            SynthBackend::System => write!(f, "系统 MIDI"),
            SynthBackend::Lgs => write!(f, "LGS (GPU)"),
        }
    }
}

/// 音频引擎后端（当前仅 Realtime：xsynth-realtime 多线程 + BufferedRenderer）
///
/// 旧配置文件可能残留 `"Core"` 取值，反序列化时通过 `#[serde(other)]` 回落到 `Realtime`，
/// 避免历史配置加载失败（ring 引擎已被整体移除）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioEngineKind {
    /// Realtime：xsynth-realtime 多线程 + BufferedRenderer（lumino 原有）
    #[default]
    #[serde(other)]
    Realtime,
}

impl std::fmt::Display for AudioEngineKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioEngineKind::Realtime => write!(f, "Realtime (xsynth)"),
        }
    }
}
