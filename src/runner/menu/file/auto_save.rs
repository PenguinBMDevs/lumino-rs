//! REND-019 PR-C：重连失败兜底前的同步自动保存（红线 9）。

use std::path::PathBuf;

use crate::runner::RunnerInner;

impl RunnerInner {
    /// Failed 兜底态触发前的**同步**自动保存（红线 9：必须先完成自动保存）。
    ///
    /// - 已有 .lmpj 源 → 覆盖保存原文件；
    /// - 否则写恢复文件 `lumino-recovery-<unix秒>.lmpj` 到文档目录（无则临时目录）。
    ///
    /// 与常规保存共用工程构建口径（`save::build_lmpj_project`），但不经 tokio——
    /// 返回时工程已落盘，随后才进入失败兜底对话框。
    pub(crate) fn auto_save_before_failed(&mut self) -> Option<PathBuf> {
        let path = self
            .midi_state
            .current_midi_source
            .clone()
            .filter(|p| super::helpers::get_file_extension(p) == "lmpj")
            .unwrap_or_else(recovery_path);

        match self.build_lmpj_project(&path) {
            Some(project) => match lumino_export::save_to_archive(&project, &path) {
                Ok(()) => {
                    tracing::info!("重连失败兜底：工程已自动保存到 {:?}", path);
                    Some(path)
                }
                Err(e) => {
                    tracing::error!("重连失败兜底：自动保存写入失败 {:?}: {e}", path);
                    None
                }
            },
            None => {
                tracing::warn!("重连失败兜底：没有可保存的工程内容");
                None
            }
        }
    }
}

/// 恢复文件路径：文档目录（无则临时目录）`lumino-recovery-<unix秒>.lmpj`。
fn recovery_path() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!("lumino-recovery-{stamp}.lmpj");
    directories::UserDirs::new()
        .and_then(|d| d.document_dir().map(|dir| dir.join(&name)))
        .unwrap_or_else(|| std::env::temp_dir().join(name))
}
