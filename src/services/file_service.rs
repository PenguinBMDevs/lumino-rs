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

impl FileService {
    pub fn new() -> Self {
        Self {}
    }

    /// 保存为 LMPJ 文件 (Lumino MIDI Project)
    ///
    /// # Arguments
    /// * `parsed` - 解析后的 MIDI 数据
    /// * `path` - 保存路径
    ///
    /// # Returns
    /// 成功返回 Ok(())，失败返回 Err(String)
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
        lumino_core::midi::loader::send_progress_message("准备导出 MIDI 文件", 0.0);
        lumino_core::midi::loader::send_progress_message("正在导出 MIDI 文件", 0.3);

        match tokio::task::spawn_blocking(move || {
            lumino_export::export_midi_from_parsed_midi_sync(&source_path)
        })
        .await
        {
            Ok(Ok(bytes)) => {
                lumino_core::midi::loader::send_progress_message("正在写入文件", 0.8);
                match std::fs::write(&path, bytes) {
                    Ok(()) => {
                        lumino_core::midi::loader::send_progress_message("MIDI 导出成功", 1.0);
                        tracing::info!("MIDI 导出成功");
                        Ok(())
                    }
                    Err(e) => {
                        lumino_core::midi::loader::send_progress_message(
                            &format!("写入文件失败: {e}"),
                            1.0,
                        );
                        tracing::error!("MIDI 导出失败: {}", e);
                        Err(e.to_string())
                    }
                }
            }
            Ok(Err(e)) => {
                lumino_core::midi::loader::send_progress_message(&format!("导出失败: {e}"), 1.0);
                tracing::error!("MIDI 导出失败: {}", e);
                Err(e.to_string())
            }
            Err(e) => {
                lumino_core::midi::loader::send_progress_message(&format!("导出失败: {e}"), 1.0);
                tracing::error!("MIDI 导出失败: {}", e);
                Err(e.to_string())
            }
        }
    }

    /// 保存为 DMS 文件
    pub async fn save_as_dms(&self, source_path: PathBuf, path: PathBuf) -> Result<(), String> {
        lumino_core::midi::loader::send_progress_message("准备导出 DMS 文件", 0.0);
        lumino_core::midi::loader::send_progress_message("正在读取 MIDI 文件", 0.2);

        match tokio::task::spawn_blocking(move || {
            lumino_core::midi::loader::send_progress_message("正在转换格式", 0.5);
            let bytes = lumino_export::export_dms_from_midi_sync(&source_path)?;
            lumino_core::midi::loader::send_progress_message("正在写入 DMS 文件", 0.8);
            std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
        })
        .await
        {
            Ok(Ok(_)) => {
                lumino_core::midi::loader::send_progress_message("MIDI 转 DMS 导出成功", 1.0);
                tracing::info!("MIDI 转 DMS 导出成功");
                Ok(())
            }
            Ok(Err(e)) => {
                lumino_core::midi::loader::send_progress_message(&format!("导出失败: {e}"), 1.0);
                tracing::error!("MIDI 转 DMS 导出失败: {}", e);
                Err(e)
            }
            Err(e) => {
                lumino_core::midi::loader::send_progress_message(&format!("导出失败: {e}"), 1.0);
                tracing::error!("MIDI 转 DMS 导出失败: {}", e);
                Err(e.to_string())
            }
        }
    }

    /// 复制 DMS 文件
    pub async fn copy_dms_file(&self, source_path: PathBuf, path: PathBuf) -> Result<(), String> {
        lumino_core::midi::loader::send_progress_message("准备保存 DMS 文件", 0.0);
        lumino_core::midi::loader::send_progress_message("正在复制 DMS 文件", 0.5);

        let path_clone = path.clone();

        match tokio::task::spawn_blocking(move || {
            lumino_export::copy_file_sync(&source_path, &path_clone)
        })
        .await
        {
            Ok(Ok(_)) => {
                lumino_core::midi::loader::send_progress_message("DMS 保存成功", 1.0);
                tracing::info!("DMS 保存成功: {:?}", path);
                Ok(())
            }
            Ok(Err(e)) => {
                lumino_core::midi::loader::send_progress_message(&format!("保存失败: {e}"), 1.0);
                tracing::error!("DMS 保存失败: {}", e);
                Err(e)
            }
            Err(e) => {
                lumino_core::midi::loader::send_progress_message(&format!("保存失败: {e}"), 1.0);
                tracing::error!("DMS 保存失败: {}", e);
                Err(e.to_string())
            }
        }
    }

    /// 导出 DMS 到 MIDI
    pub async fn export_dms_to_midi(
        &self,
        source_path: PathBuf,
        path: PathBuf,
    ) -> Result<(), String> {
        lumino_core::midi::loader::send_progress_message("准备导出 MIDI 文件", 0.0);
        lumino_core::midi::loader::send_progress_message("正在读取 DMS 文件", 0.2);

        match tokio::task::spawn_blocking(move || {
            let bytes = lumino_export::export_midi_from_dms_sync(&source_path)?;
            lumino_core::midi::loader::send_progress_message("正在写入 MIDI 文件", 0.8);
            std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
        })
        .await
        {
            Ok(Ok(_)) => {
                lumino_core::midi::loader::send_progress_message("DMS 转 MIDI 导出成功", 1.0);
                tracing::info!("DMS 转 MIDI 导出成功");
                Ok(())
            }
            Ok(Err(e)) => {
                lumino_core::midi::loader::send_progress_message(&format!("导出失败: {e}"), 1.0);
                tracing::error!("DMS 转 MIDI 导出失败: {}", e);
                Err(e)
            }
            Err(e) => {
                lumino_core::midi::loader::send_progress_message(&format!("导出失败: {e}"), 1.0);
                tracing::error!("DMS 转 MIDI 导出失败: {}", e);
                Err(e.to_string())
            }
        }
    }
}
