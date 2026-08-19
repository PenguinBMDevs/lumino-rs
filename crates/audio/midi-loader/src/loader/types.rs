use std::sync::Arc;

use tokio::sync::mpsc as tokio_mpsc;

/// 进度回调函数类型：(消息, 进度 0.0~1.0)
/// 使用 Arc 包装以便跨线程共享和克隆
pub type ProgressCallback = Arc<dyn Fn(&str, f64) + Send + Sync>;

/// 从 tokio unbounded sender 创建进度回调（供应用层使用）
pub fn progress_from_sender(
    sender: tokio_mpsc::UnboundedSender<(String, f64)>,
) -> ProgressCallback {
    Arc::new(move |message: &str, progress: f64| {
        let _ = sender.send((message.to_string(), progress.clamp(0.0, 1.0)));
    })
}

/// 无操作的进度回调（静默模式）
pub fn silent_progress() -> ProgressCallback {
    Arc::new(|_, _| {})
}
