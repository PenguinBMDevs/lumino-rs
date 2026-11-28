//! 进度通道与导出控制句柄的创建。

use std::sync::Arc;

use crate::runner::RunnerInner;
use lumino_export::audio::config::ProgressCallback;
use lumino_export::audio::control::AudioExportControl;

/// 进度消息载荷：(消息, 进度, 当前帧, 总帧, 已用秒)。
pub(super) type ProgressMsg = (String, f64, u64, f64, f64);

/// 启动导出时创建的通道三元组。
pub(super) type ExportChannels = (
    tokio::sync::mpsc::UnboundedSender<ProgressMsg>,
    Arc<AudioExportControl>,
    ProgressCallback,
);

impl RunnerInner {
    /// 创建进度通道与控制句柄，并写入窗口状态供主线程消费。
    pub(super) fn prepare_export_channels(&mut self) -> ExportChannels {
        let (progress_tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();
        self.window_state.export_progress_rx = Some(progress_rx);

        let control = Arc::new(AudioExportControl::new());
        self.window_state.audio_export_control = Some(Arc::clone(&control));

        let progress_tx_cb = progress_tx.clone();
        let progress_cb: ProgressCallback = Arc::new(move |msg: String, pct: f64| {
            let _ = progress_tx_cb.send((msg, pct, 0, 0.0, 0.0));
        });

        (progress_tx, control, progress_cb)
    }
}
