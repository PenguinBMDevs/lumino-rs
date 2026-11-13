//! 视频导出进度覆盖层的 egui 模型。
//!
//! 对应 旧版 `view/video_export_dialog/handlers.rs` 的四态：导出中 / 编码收尾 /
//! 完成 / 失败。所有文案（含已格式化的时间、帧数、倍率）由生产侧预计算，
//! 渲染层只负责排版。

/// 覆盖层视图（`None` 表示不显示）。
#[derive(Debug, Clone, PartialEq)]
pub enum VideoOverlayView {
    /// 导出中（渲染 + 写帧）。
    Exporting(VideoOverlayProgress),
    /// 编码收尾（等待 ffmpeg 封装）。
    Finalizing(VideoOverlayFinalizing),
    /// 完成。
    Completed(VideoOverlayCompleted),
    /// 失败。
    Error(VideoOverlayError),
}

impl Default for VideoOverlayView {
    fn default() -> Self {
        Self::Exporting(VideoOverlayProgress::default())
    }
}

/// 导出中 / 收尾共用的进度块。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VideoOverlayProgress {
    /// 覆盖层标题（「视频导出中」）。
    pub heading: String,
    /// 「帧: {current} / {total}」。
    pub frames_line: String,
    /// 「时间: {elapsed} / {total}」。
    pub time_line: String,
    /// 进度 0..=1。
    pub progress: f32,
    /// 「{:.1}%」。
    pub percent_line: String,
    /// 「渲染速度: {} fps」。
    pub render_fps_line: String,
    /// 「速度: {}x 原速」。
    pub speed_line: String,
    /// 剩余时间提示。
    pub remaining_line: String,
}

/// 收尾态：进度块 + 说明 + 强制完成。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VideoOverlayFinalizing {
    /// 共用进度块。
    pub progress: VideoOverlayProgress,
    /// 「正在完成编码...」（14px）。
    pub finalizing_line: String,
    /// 「ffmpeg 正在封装文件，请稍候…」（12px 弱色）。
    pub hint: String,
    /// 「强制完成」按钮。
    pub force_finish: String,
    /// 「视频已可用，跳过等待」（12px 弱色）。
    pub force_hint: String,
}

/// 完成态。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VideoOverlayCompleted {
    /// 「导出完成！」（16px 成功色）。
    pub heading: String,
    /// 统计行（总帧数 / 时长 / 总用时 / 平均速度 / 倍率，13px 弱色）。
    pub stats: Vec<String>,
    /// 「确定」按钮。
    pub confirm: String,
}

/// 失败态。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VideoOverlayError {
    /// 「导出失败」（16px 危险色）。
    pub heading: String,
    /// 错误详情（可滚动）。
    pub detail: String,
    /// 「确定」按钮。
    pub confirm: String,
}

/// 整个覆盖层模型。
#[derive(Debug, Clone, Default)]
pub struct VideoExportOverlayModel {
    /// 顶部是否有 30px 旧版 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 当前状态。
    pub view: VideoOverlayView,
}

/// 覆盖层动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoOverlayAction {
    /// 取消导出（导出中）。
    CancelExport,
    /// 强制完成（编码收尾）。
    ForceFinish,
    /// 关闭覆盖层（完成 / 失败）。
    Dismiss,
}
