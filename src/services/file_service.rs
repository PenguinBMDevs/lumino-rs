use lumino_midi_loader::loader::ProgressCallback;
use std::path::PathBuf;
use std::sync::Arc;

/// 文件服务 - 处理所有文件操作
///
/// 该服务负责处理文件的保存、加载和导出操作，
/// 包括 LMPJ、MIDI 和 DMS 格式的文件处理。
#[derive(Clone)]
pub struct FileService {
    /// 进度回调（依赖注入）
    progress_cb: Arc<ProgressCallback>,
}

impl FileService {
    pub fn new(progress_cb: ProgressCallback) -> Self {
        Self {
            progress_cb: Arc::new(progress_cb),
        }
    }

    /// 在后台线程执行阻塞文件操作，带进度反馈
    async fn run_blocking_task(
        &self,
        start_msg: &'static str,
        active_msg: &'static str,
        done_msg: &'static str,
        log_done_msg: &'static str,
        task: impl FnOnce(ProgressCallback) -> Result<(), String> + Send + 'static,
    ) -> Result<(), String> {
        let cb: ProgressCallback = (*self.progress_cb).clone();
        cb(start_msg, 0.0);
        cb(active_msg, 0.3);

        let cb_for_blocking = cb.clone();
        match tokio::task::spawn_blocking(move || task(cb_for_blocking)).await {
            Ok(Ok(())) => {
                cb(done_msg, 1.0);
                tracing::info!("{}", log_done_msg);
                Ok(())
            }
            Ok(Err(e)) => {
                let msg = e.to_string();
                cb(&msg, 1.0);
                tracing::error!("{}", msg);
                Err(e)
            }
            Err(e) => {
                let msg = format!("{}", e);
                cb(&msg, 1.0);
                tracing::error!("{}", msg);
                Err(msg)
            }
        }
    }

    /// 保存为 MIDI 文件
    pub async fn save_as_midi(&self, source_path: PathBuf, path: PathBuf) -> Result<(), String> {
        self.run_blocking_task(
            "准备导出 MIDI 文件",
            "正在导出 MIDI 文件",
            "MIDI 导出成功",
            "MIDI 导出成功",
            move |cb| {
                let bytes = lumino_export::export_midi_from_parsed_midi_sync(&source_path)
                    .map_err(|e| e.to_string())?;
                cb("正在写入文件", 0.8);
                std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
            },
        )
        .await
    }

    /// 导出 DMS 到 MIDI
    pub async fn export_dms_to_midi(
        &self,
        source_path: PathBuf,
        path: PathBuf,
    ) -> Result<(), String> {
        self.run_blocking_task(
            "准备导出 MIDI 文件",
            "正在读取 DMS 文件",
            "DMS 转 MIDI 导出成功",
            "DMS 转 MIDI 导出成功",
            move |cb| {
                let bytes = lumino_export::export_midi_from_dms_sync(&source_path)
                    .map_err(|e| e.to_string())?;
                cb("正在写入 MIDI 文件", 0.8);
                std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
            },
        )
        .await
    }
}
