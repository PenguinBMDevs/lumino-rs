//! 视频导出错误类型

use thiserror::Error;

/// 视频导出错误
#[derive(Debug, Error)]
pub enum VideoExportError {
    /// IO 错误
    #[error("{}: {}", lumino_core::i18n::tr("IO 错误"), .0)]
    Io(#[from] std::io::Error),

    /// ffmpeg 进程退出码非零
    #[error("{} {:?}", lumino_core::i18n::tr("ffmpeg 退出码"), .0)]
    FfmpegFailed(Option<i32>),

    /// ffmpeg 写入失败（含 stderr 上下文）
    #[error("{}: {}", lumino_core::i18n::tr("ffmpeg 写入失败"), .0)]
    FfmpegWriteFailed(String),

    /// 未找到 ffmpeg 可执行文件
    #[error("{}", lumino_core::i18n::tr("未找到 ffmpeg 可执行文件"))]
    FfmpegNotFound,

    /// 子进程管道建立失败（stdin/stdout/stderr 未被正确重定向）
    #[error("{}: {}", lumino_core::i18n::tr("ffmpeg 管道建立失败"), .0)]
    PipeSetupFailed(String),

    /// 帧数据尺寸不匹配
    #[error(
        "{}: got {} bytes, expected {} bytes",
        lumino_core::i18n::tr("帧数据尺寸不匹配"),
        .got,
        .expected
    )]
    FrameSizeMismatch {
        /// 实际接收的字节数
        got: usize,
        /// 期望的字节数
        expected: usize,
    },
}

/// 视频导出结果类型
pub type VideoExportResult<T> = Result<T, VideoExportError>;
