//! 音频导出动作

use crate::{AudioChannels, AudioFormat, Interpolation, ThreadingOption};

/// 音频导出动作
#[derive(Debug, Clone)]
pub enum AudioExportAction {
    /// 打开音频导出对话框
    OpenDialog,
    /// 关闭音频导出对话框
    CloseDialog,
    /// 确认音频导出
    Confirm,
    /// 取消音频导出
    Cancel,
    /// 工程名称变更
    ProjectNameChanged(String),
    /// 输出格式变更
    FormatChanged(AudioFormat),
    /// 采样率变更
    SampleRateChanged(u32),
    /// 通道数变更
    ChannelsChanged(AudioChannels),
    /// 层数限制变更
    LayersChanged(String),
    /// 通道多线程变更
    ChannelThreadingChanged(ThreadingOption),
    /// 按键多线程变更
    KeyThreadingChanged(ThreadingOption),
    /// 插值算法变更
    InterpolationChanged(Interpolation),
    /// 应用限制器变更
    ApplyLimiterChanged(bool),
    /// 禁用淡出变更
    DisableFadeOutChanged(bool),
    /// 线性包络变更
    LinearEnvelopeChanged(bool),
    /// 输出路径变更
    OutputPathChanged(String),
    /// 浏览输出路径
    BrowseOutput,
    /// 浏览 MIDI 文件
    BrowseMidi,
    /// 浏览音色库文件
    BrowseSoundfont,
    /// 进度更新
    Progress(f32, String),
    /// 完成
    Completed,
    /// 失败
    Failed(String),
}
