//! 视频导出动作 — UI 控件交互与 Runner 回调
//!
//! 配置值用 String 传递（UI pick_list 原生支持），
//! Runner 端解析回强类型（与音频导出的 threading/interpolation 解析方式一致）。

/// 视频导出动作
#[derive(Debug, Clone)]
pub enum VideoExportAction {
    // ── 面板控制 ──
    /// 打开视频导出面板（侧边栏视频渲染子按钮）
    OpenPanel,
    /// 关闭视频导出面板
    ClosePanel,

    // ── 配置变更 ──
    /// 容器格式变更（"MP4"/"MOV"/"MKV"/"AVI"）
    ContainerChanged(String),
    /// 视频编码器变更（"H.264"/"H.265 / HEVC"/"ProRes"/"VP9"/"AV1"）
    CodecChanged(String),
    /// 硬件加速后端变更（"Software (CPU)"/"NVENC (NVIDIA)" 等）
    BackendChanged(String),
    /// 质量预设变更（"高"/"中"/"低"）
    QualityChanged(String),
    /// 分辨率宽度变更
    WidthChanged(String),
    /// 分辨率高度变更
    HeightChanged(String),
    /// 帧率变更
    FpsChanged(u32),
    /// 输出路径变更
    OutputPathChanged(String),
    /// 浏览输出路径
    BrowseOutput,
    /// MIDI 路径变更
    MidiPathChanged(String),
    /// 浏览 MIDI 路径
    BrowseMidi,

    // ── 导出控制 ──
    /// 开始导出（点击「开始导出」按钮，由 handler 发射 StartVideoExport 事件）
    StartExport,
    /// 取消导出（Exporting 状态的「取消」按钮）
    CancelExport,
    /// 强制完成（Finalizing 状态的「强制完成」按钮，跳过等待 ffmpeg 封装）
    ForceFinish,
    /// 关闭覆盖层（Completed/Error 状态的「确定」按钮）
    DismissOverlay,

    // ── Runner 回调 ──
    /// 更新导出进度
    UpdateProgress {
        /// 状态消息
        message: String,
        /// 进度 0.0-1.0
        progress: f64,
        /// 当前帧
        current_frame: u64,
        /// 总帧数
        total_frames: u64,
        /// 渲染速度（fps）
        fps: f64,
    },
    /// 更新预览帧（RGBA 像素数据，含宽高）
    UpdatePreviewFrame {
        /// RGBA 像素数据
        data: Vec<u8>,
        /// 图像宽度
        width: u32,
        /// 图像高度
        height: u32,
    },
    /// 导出完成
    ExportCompleted,
    /// 导出失败
    ExportFailed(String),
}
