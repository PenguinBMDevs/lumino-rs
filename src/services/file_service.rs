use lumino_core::ParsedMidi;
use std::path::PathBuf;

/// 文件服务 - 处理所有文件操作
///
/// 该服务负责处理文件的保存、加载和导出操作，
/// 包括 LMPJ、MIDI 和 DMS 格式的文件处理。
#[derive(Clone)]
pub struct FileService {
    // 可以在这里添加依赖，如配置、日志等
}

/// 辅助宏：统一处理 spawn_blocking 的结果，包括进度消息和日志。
macro_rules! spawn_blocking_with_progress {
    (
        $progress_start:expr,       // 开始进度消息 (&str)
        $progress_active:expr,      // 进行中进度消息 (&str)
        $progress_done:expr,        // 完成进度消息 (&str)
        $log_done:expr,             // info 日志消息 (&str)
        $blocking_closure:expr $(,)? // spawn_blocking 闭包 (返回 impl Into<String> 或 Result<T, impl Into<String>>)
    ) => {
        {
            lumino_core::midi::loader::send_progress_message($progress_start, 0.0);
            lumino_core::midi::loader::send_progress_message($progress_active, 0.3);

            match tokio::task::spawn_blocking($blocking_closure).await {
                Ok(Ok(_)) => {
                    lumino_core::midi::loader::send_progress_message($progress_done, 1.0);
                    tracing::info!($log_done);
                    Ok(())
                }
                Ok(Err(e)) => {
                    let msg = format!("{}", e);
                    lumino_core::midi::loader::send_progress_message(&msg, 1.0);
                    tracing::error!("{}", msg);
                    Err(e)
                }
                Err(e) => {
                    let msg = format!("{}", e);
                    lumino_core::midi::loader::send_progress_message(&msg, 1.0);
                    tracing::error!("{}", msg);
                    Err(msg)
                }
            }
        }
    };
}

impl FileService {
    pub fn new() -> Self {
        Self {}
    }

    /// 保存为 LMPJ 文件 (Lumino MIDI Project)
    pub async fn save_as_lmpj(&self, parsed: &ParsedMidi, path: PathBuf) -> Result<(), String> {
        lumino_core::midi::loader::send_progress_message("准备保存 LMPJ 文件", 0.0);
        lumino_core::midi::loader::send_progress_message("正在保存 LMPJ 文件", 0.3);

        match lumino_export::save(parsed, path.clone()).await {
            Ok(()) => {
                lumino_core::midi::loader::send_progress_message("LMPJ 保存成功", 1.0);
                tracing::info!("MIDI保存成功: {:?}", path);
                Ok(())
            }
            Err(e) => {
                lumino_core::midi::loader::send_progress_message(&format!("保存失败: {e}"), 1.0);
                tracing::error!("MIDI保存失败: {}", e);
                Err(e.to_string())
            }
        }
    }

    /// 保存为 MIDI 文件
    pub async fn save_as_midi(&self, source_path: PathBuf, path: PathBuf) -> Result<(), String> {
        spawn_blocking_with_progress!(
            "准备导出 MIDI 文件",
            "正在导出 MIDI 文件",
            "MIDI 导出成功",
            "MIDI 导出成功",
            move || -> Result<(), String> {
                let bytes = lumino_export::export_midi_from_parsed_midi_sync(&source_path)?;
                lumino_core::midi::loader::send_progress_message("正在写入文件", 0.8);
                std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
            }
        )
    }

    /// 保存为 DMS 文件
    pub async fn save_as_dms(&self, source_path: PathBuf, path: PathBuf) -> Result<(), String> {
        spawn_blocking_with_progress!(
            "准备导出 DMS 文件",
            "正在读取 MIDI 文件",
            "MIDI 转 DMS 导出成功",
            "MIDI 转 DMS 导出成功",
            move || -> Result<(), String> {
                lumino_core::midi::loader::send_progress_message("正在转换格式", 0.5);
                let bytes = lumino_export::export_dms_from_midi_sync(&source_path)?;
                lumino_core::midi::loader::send_progress_message("正在写入 DMS 文件", 0.8);
                std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
            }
        )
    }

    /// 复制 DMS 文件
    pub async fn copy_dms_file(&self, source_path: PathBuf, path: PathBuf) -> Result<(), String> {
        spawn_blocking_with_progress!(
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
        spawn_blocking_with_progress!(
            "准备导出 MIDI 文件",
            "正在读取 DMS 文件",
            "DMS 转 MIDI 导出成功",
            "DMS 转 MIDI 导出成功",
            move || -> Result<(), String> {
                let bytes = lumino_export::export_midi_from_dms_sync(&source_path)?;
                lumino_core::midi::loader::send_progress_message("正在写入 MIDI 文件", 0.8);
                std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
            }
        )
    }
}
