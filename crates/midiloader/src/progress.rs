use crossbeam_channel::{Receiver, Sender, bounded};

/// 进度信息
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    /// 已读取的字节数
    pub bytes_read: u64,
    /// 总字节数
    pub total_bytes: u64,
    /// 已解析的事件数
    pub events_parsed: u64,
    /// 已解析的轨道数
    pub tracks_parsed: u16,
    /// 总轨道数
    pub total_tracks: u16,
}

impl Progress {
    /// 计算完成百分比
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.bytes_read as f64 / self.total_bytes as f64) * 100.0
        }
    }

    /// 检查是否已完成
    pub fn is_complete(&self) -> bool {
        self.bytes_read >= self.total_bytes
    }

    /// 计算轨道完成百分比
    pub fn track_percentage(&self) -> f64 {
        if self.total_tracks == 0 {
            0.0
        } else {
            (self.tracks_parsed as f64 / self.total_tracks as f64) * 100.0
        }
    }
}

/// 进度事件类型
#[derive(Debug, Clone, PartialEq)]
pub enum ProgressEvent {
    /// 开始加载
    Started { total_bytes: u64 },
    /// 进度更新
    Progress(Progress),
    /// 轨道解析完成
    TrackComplete { track_index: u16, event_count: u64 },
    /// 加载完成
    Completed,
    /// 发生错误
    Error(String),
}

/// 进度报告器，用于向外部报告加载进度
#[derive(Debug, Clone)]
pub struct ProgressReporter {
    sender: Sender<ProgressEvent>,
}

impl ProgressReporter {
    /// 创建新的进度报告器
    pub fn new(sender: Sender<ProgressEvent>) -> Self {
        Self { sender }
    }

    /// 报告事件
    pub fn report(&self, event: ProgressEvent) {
        let _ = self.sender.try_send(event);
    }

    /// 报告开始加载
    pub fn started(&self, total_bytes: u64) {
        self.report(ProgressEvent::Started { total_bytes });
    }

    /// 报告进度
    pub fn progress(&self, progress: Progress) {
        self.report(ProgressEvent::Progress(progress));
    }

    /// 报告轨道完成
    pub fn track_complete(&self, track_index: u16, event_count: u64) {
        self.report(ProgressEvent::TrackComplete {
            track_index,
            event_count,
        });
    }

    /// 报告加载完成
    pub fn completed(&self) {
        self.report(ProgressEvent::Completed);
    }

    /// 报告错误
    pub fn error(&self, msg: String) {
        self.report(ProgressEvent::Error(msg));
    }
}

/// 进度句柄，用于接收进度事件
#[derive(Debug)]
pub struct ProgressHandle {
    receiver: Receiver<ProgressEvent>,
}

impl ProgressHandle {
    /// 创建新的进度句柄和报告器
    ///
    /// # 参数
    ///
    /// * `capacity` - 通道缓冲区容量
    ///
    /// # 返回
    ///
    /// 返回 `(ProgressHandle, ProgressReporter)` 元组
    pub fn new(capacity: usize) -> (Self, ProgressReporter) {
        let (sender, receiver) = bounded(capacity);
        (Self { receiver }, ProgressReporter::new(sender))
    }

    /// 获取接收器的引用
    pub fn receiver(&self) -> &Receiver<ProgressEvent> {
        &self.receiver
    }

    /// 尝试接收事件（非阻塞）
    pub fn try_recv(&self) -> Option<ProgressEvent> {
        self.receiver.try_recv().ok()
    }

    /// 接收事件（阻塞）
    pub fn recv(&self) -> Result<ProgressEvent, crossbeam_channel::RecvError> {
        self.receiver.recv()
    }
}

impl Default for ProgressHandle {
    fn default() -> Self {
        let (handle, _) = Self::new(1024);
        handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_percentage() {
        let progress = Progress {
            bytes_read: 50,
            total_bytes: 100,
            events_parsed: 10,
            tracks_parsed: 1,
            total_tracks: 2,
        };

        assert_eq!(progress.percentage(), 50.0);
        assert_eq!(progress.track_percentage(), 50.0);
        assert!(!progress.is_complete());

        let complete = Progress {
            bytes_read: 100,
            total_bytes: 100,
            events_parsed: 20,
            tracks_parsed: 2,
            total_tracks: 2,
        };

        assert!(complete.is_complete());
    }

    #[test]
    fn test_progress_zero_division() {
        let progress = Progress {
            bytes_read: 0,
            total_bytes: 0,
            events_parsed: 0,
            tracks_parsed: 0,
            total_tracks: 0,
        };

        assert_eq!(progress.percentage(), 0.0);
        assert_eq!(progress.track_percentage(), 0.0);
    }

    #[test]
    fn test_progress_handle() {
        let (handle, reporter) = ProgressHandle::new(1024);

        reporter.started(100);
        reporter.progress(Progress {
            bytes_read: 50,
            total_bytes: 100,
            events_parsed: 10,
            tracks_parsed: 1,
            total_tracks: 2,
        });
        reporter.completed();

        assert!(matches!(
            handle.recv(),
            Ok(ProgressEvent::Started { total_bytes: 100 })
        ));
        assert!(matches!(handle.recv(), Ok(ProgressEvent::Progress(_))));
        assert!(matches!(handle.recv(), Ok(ProgressEvent::Completed)));
    }

    #[test]
    fn test_progress_reporter_error() {
        let (handle, reporter) = ProgressHandle::new(1024);

        reporter.error("Test error".to_string());

        if let Ok(ProgressEvent::Error(msg)) = handle.recv() {
            assert_eq!(msg, "Test error");
        } else {
            panic!("Expected Error event");
        }
    }
}
