//! 播放状态枚举

/// 播放状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    /// 停止
    Stopped,
    /// 播放中
    Playing,
    /// 暂停
    Paused,
}
