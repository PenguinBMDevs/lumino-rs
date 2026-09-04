//! 音频导出动作 — 仅保留 UI 控件交互变体

use crate::{AudioBackend, AudioChannels, AudioFormat, Interpolation, ThreadingOption};

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
    /// 编码比特率变更（kbps，仅 MP3/Vorbis）
    BitrateChanged(String),
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
    /// 忽略音色变化事件
    IgnoreProgramChangesChanged(bool),
    /// 启用音符力度过滤
    FilterVelocityChanged(bool),
    /// 最低力度变更
    VelocityLowChanged(String),
    /// 最高力度变更
    VelocityHighChanged(String),
    /// 启用键位过滤
    FilterKeyChanged(bool),
    /// 最低键位变更
    KeyLowChanged(String),
    /// 最高键位变更
    KeyHighChanged(String),
    /// 音符强制结束延迟（毫秒）
    NoteForceEndDelayChanged(String),
    /// 渲染后端变更（CPU / GPU）
    BackendChanged(AudioBackend),
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
    UpdateRenderProgress {
        /// 渲染进度消息文本
        message: String,
        /// 渲染进度（0.0 到 1.0）
        progress: f64,
    },
    /// 渲染完成
    RenderCompleted,
    /// 渲染失败
    RenderFailed(String),
    /// 重置渲染状态
    ResetRendering,
    /// 暂停导出
    Pause,
    /// 继续导出
    Resume,
    /// 切换暂停/继续
    TogglePause,
    /// 中止导出
    Abort,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_export_action_variants() {
        let action = AudioExportAction::OpenPanel;
        assert!(matches!(action, AudioExportAction::OpenPanel));

        let action = AudioExportAction::ClosePanel;
        assert!(matches!(action, AudioExportAction::ClosePanel));

        let action = AudioExportAction::BitrateChanged("320".to_string());
        assert!(matches!(action, AudioExportAction::BitrateChanged(_)));

        let action = AudioExportAction::IgnoreProgramChangesChanged(true);
        assert!(matches!(
            action,
            AudioExportAction::IgnoreProgramChangesChanged(_)
        ));

        let action = AudioExportAction::FilterVelocityChanged(true);
        assert!(matches!(
            action,
            AudioExportAction::FilterVelocityChanged(_)
        ));

        let action = AudioExportAction::FilterKeyChanged(true);
        assert!(matches!(action, AudioExportAction::FilterKeyChanged(_)));
    }
}
