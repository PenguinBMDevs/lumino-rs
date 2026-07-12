//! 音频导出动作 — 仅保留 UI 控件交互变体

use crate::{AudioChannels, AudioFormat, Interpolation, ThreadingOption};

/// 音频导出动作
#[derive(Debug, Clone)]
pub enum AudioExportAction {
    /// 打开音频导出面板（主界面侧边栏面板）
    OpenPanel,
    /// 关闭音频导出面板（返回主编辑器）
    ClosePanel,
    /// 确认音频导出（由重写的 handler 处理）
    Confirm,
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
    /// GPU 加速渲染变更
    UseGpuChanged(bool),
    /// 输出路径变更
    OutputPathChanged(String),
    /// 浏览输出路径
    BrowseOutput,
    /// 浏览 MIDI 文件
    BrowseMidi,
    /// 浏览音色库文件
    BrowseSoundfont,
    /// 开始渲染（UI 立即显示进度条）
    StartRendering,
    /// 更新渲染进度
    UpdateRenderProgress { message: String, progress: f64 },
    /// 渲染完成
    RenderCompleted,
    /// 渲染失败
    RenderFailed(String),
    /// 重置渲染状态
    ResetRendering,
}
