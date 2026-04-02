use lumino_core::ParsedMidi;
use lumino_core::midi::loader::ProgressCallback;
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

/// 辅助宏：统一处理 spawn_blocking 的结果，包括进度消息和日志。
///
/// # 参数
/// - `$progress_cb`: 必须是 owned 的 `ProgressCallback`（即 `Arc<dyn Fn(...)>`），将被 move 进闭包
macro_rules! spawn_blocking_with_progress {
    (
        $progress_cb:expr,       // owned 进度回调（会被 move 进闭包）
        $progress_start:expr,       // 开始进度消息 (&str)
        $progress_active:expr,      // 进行中进度消息 (&str)
        $progress_done:expr,        // 完成进度消息 (&str)
        $log_done:expr,             // info 日志消息 (&str)
        $blocking_closure:expr $(,)? // spawn_blocking 闭包 (返回 Result<T, impl Into<String>>)
    ) => {{
        let cb = $progress_cb;
        cb($progress_start, 0.0);
        cb($progress_active, 0.3);

        // 用户闭包可能是 move 闭包，会提前 capture cb，
        // 所以先 clone 一份给 spawn_blocking 内部使用
        let cb_for_blocking = cb.clone();
        let blocking_fn = $blocking_closure;
        match tokio::task::spawn_blocking(move || blocking_fn()).await {
            Ok(Ok(_)) => {
                cb_for_blocking($progress_done, 1.0);
                tracing::info!($log_done);
                Ok(())
            }
            Ok(Err(e)) => {
                let msg = format!("{}", e);
                cb_for_blocking(&msg, 1.0);
                tracing::error!("{}", msg);
                Err(e)
            }
            Err(e) => {
                let msg = format!("{}", e);
                cb_for_blocking(&msg, 1.0);
                tracing::error!("{}", msg);
                Err(msg)
            }
        }
    }};
}

impl FileService {
    pub fn new(progress_cb: ProgressCallback) -> Self {
        Self {
            progress_cb: Arc::new(progress_cb),
        }
    }

    /// 保存为 LMPJ 文件 (Lumino MIDI Project)
    pub async fn save_as_lmpj(&self, parsed: &ParsedMidi, path: PathBuf) -> Result<(), String> {
        let cb = &*self.progress_cb;
        cb("准备保存 LMPJ 文件", 0.0);
        cb("正在保存 LMPJ 文件", 0.3);

        match lumino_export::save(parsed, path.clone()).await {
            Ok(()) => {
                cb("LMPJ 保存成功", 1.0);
                tracing::info!("MIDI保存成功: {:?}", path);
                Ok(())
            }
            Err(e) => {
                cb(&format!("保存失败: {e}"), 1.0);
                tracing::error!("MIDI保存失败: {}", e);
                Err(e.to_string())
            }
        }
    }

    /// 保存为 MIDI 文件
    pub async fn save_as_midi(&self, source_path: PathBuf, path: PathBuf) -> Result<(), String> {
        let cb = Arc::clone(&self.progress_cb);
        spawn_blocking_with_progress!(
            cb.clone(),
            "准备导出 MIDI 文件",
            "正在导出 MIDI 文件",
            "MIDI 导出成功",
            "MIDI 导出成功",
            move || -> Result<(), String> {
                let bytes = lumino_export::export_midi_from_parsed_midi_sync(&source_path)?;
                cb("正在写入文件", 0.8);
                std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
            }
        )
    }

    /// 保存为 DMS 文件
    pub async fn save_as_dms(&self, source_path: PathBuf, path: PathBuf) -> Result<(), String> {
        let cb = Arc::clone(&self.progress_cb);
        spawn_blocking_with_progress!(
            cb.clone(),
            "准备导出 DMS 文件",
            "正在读取 MIDI 文件",
            "MIDI 转 DMS 导出成功",
            "MIDI 转 DMS 导出成功",
            move || -> Result<(), String> {
                cb("正在转换格式", 0.5);
                let bytes = lumino_export::export_dms_from_midi_sync(&source_path)?;
                cb("正在写入 DMS 文件", 0.8);
                std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
            }
        )
    }

    /// 复制 DMS 文件
    pub async fn copy_dms_file(&self, source_path: PathBuf, path: PathBuf) -> Result<(), String> {
        let cb = Arc::clone(&self.progress_cb);
        spawn_blocking_with_progress!(
            cb.clone(),
            "准备保存 DMS 文件",
            "正在复制 DMS 文件",
            "DMS 保存成功",
            "DMS 保存成功",
            move || -> Result<(), String> {
                lumino_export::copy_file_sync(&source_path, &path).map(|_| ())
            }
        )
    }

    /// 导出 DMS 到 MIDI
    pub async fn export_dms_to_midi(
        &self,
        source_path: PathBuf,
        path: PathBuf,
    ) -> Result<(), String> {
        let cb = Arc::clone(&self.progress_cb);
        spawn_blocking_with_progress!(
            cb.clone(),
            "准备导出 MIDI 文件",
            "正在读取 DMS 文件",
            "DMS 转 MIDI 导出成功",
            "DMS 转 MIDI 导出成功",
            move || -> Result<(), String> {
                let bytes = lumino_export::export_midi_from_dms_sync(&source_path)?;
                cb("正在写入 MIDI 文件", 0.8);
                std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
            }
        )
    }
}
