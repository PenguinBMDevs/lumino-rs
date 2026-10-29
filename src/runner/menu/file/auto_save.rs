//! 自动保存（两类，共用工程构建口径）：
//!
//! 1. **兜底同步保存**（REND-019 红线 9）：协作重连失败兜底态触发前，先同步
//!    落盘一次当前工程，随后才进入失败对话框。
//! 2. **周期自动保存**（默认关闭；设置 → 编辑 → 自动保存）：按用户设定间隔，
//!    把有未保存更改的工程异步写入独立文件（配置目录 `autosave/autosave.lmpj`）。
//!    崩溃/强杀后文件留存，正常退出时清理。
//!
//! 周期自动保存与手动保存/云上传共用 `is_saving()` / `is_cloud_saving()` 串行
//! 纪律；写入任务本身在后台完成（快照 + 组包 + 落盘），不阻塞 UI。

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use lumino_core::tr;

use crate::runner::RunnerInner;
use crate::storage;

mod recovery;

pub(crate) use recovery::AutosaveRecovery;

/// 自动保存子目录（配置目录下；与 cloud_tmp / deleted_tracks 同一套布局约定）。
const AUTOSAVE_DIR_NAME: &str = "autosave";
/// 自动保存主文件名（单槽位；原子替换整体覆盖旧内容）。
const AUTOSAVE_FILE_NAME: &str = "autosave.lmpj";
/// 自动保存备份文件名（后续恢复流程使用；普通 `.lmpj`，可被 Open project 打开）。
const AUTOSAVE_BACKUP_FILE_NAME: &str = "autosave-backup.lmpj";

/// 周期自动保存决策：同时满足以下条件才执行写入：
/// - 开关开启；
/// - 工程有未保存更改；
/// - 没有上一轮自动保存任务在飞；
/// - 手动保存 / 云上传未进行中（串行纪律）。
pub(crate) fn autosave_due(
    enabled: bool,
    modified: bool,
    in_flight: bool,
    saving: bool,
    cloud_saving: bool,
) -> bool {
    enabled && modified && !in_flight && !saving && !cloud_saving
}

/// 下次触发时刻：`now + interval`（间隔由配置层收敛到 1..=120 分钟）。
fn next_autosave_at(now: Instant, interval_mins: u32) -> Instant {
    now + Duration::from_secs(u64::from(interval_mins) * 60)
}

/// 自动保存主文件路径：配置目录 `autosave/autosave.lmpj`。
pub(crate) fn autosave_path() -> PathBuf {
    storage::config_dir()
        .join(AUTOSAVE_DIR_NAME)
        .join(AUTOSAVE_FILE_NAME)
}

/// 自动保存备份路径（普通 `.lmpj`，供 Open project 直接打开）。
pub(crate) fn autosave_backup_path() -> PathBuf {
    storage::config_dir()
        .join(AUTOSAVE_DIR_NAME)
        .join(AUTOSAVE_BACKUP_FILE_NAME)
}

impl RunnerInner {
    /// 周期自动保存心跳：`about_to_wait` 每轮调用，未启用 / 未到点时廉价早退。
    ///
    /// 到点先重排下一轮（周期从本次时刻起算），再按 [`autosave_due`] 决定是否
    /// 真正写入。未启用时清空排程，`control_flow` 不再产生额外唤醒。
    pub(crate) fn autosave_tick(&mut self) {
        // 写入失败每会话只提示一次；后台任务只置位，不直接碰 UI。
        if self.autosave_failed.swap(false, Ordering::SeqCst) && !self.autosave_notice_shown {
            self.autosave_notice_shown = true;
            self.window_state
                .window
                .ui_mut()
                .set_status_message(Some(tr!("自动保存失败，请检查日志").to_string()));
        }

        let (enabled, interval_mins) = {
            let ui = &self.window_state.storage.config.get().ui;
            (ui.autosave_enabled, ui.effective_autosave_interval_mins())
        };
        if !enabled {
            self.autosave_next = None;
            return;
        }

        let now = Instant::now();
        if self.autosave_next.is_some_and(|deadline| now < deadline) {
            return;
        }
        self.autosave_next = Some(next_autosave_at(now, interval_mins));

        let modified = self.window_state.window.ui().is_project_modified();
        let in_flight = self.autosave_in_flight.load(Ordering::SeqCst);
        let saving = self.is_saving();
        let cloud_saving = self.is_cloud_saving();
        if !autosave_due(true, modified, in_flight, saving, cloud_saving) {
            return;
        }

        if !self.spawn_autosave_lmpj(autosave_path()) {
            tracing::debug!("周期自动保存本轮未启动（无文档或上一轮仍在写）");
        }
    }

    /// 正常退出时清理自动保存文件。
    ///
    /// 若写入任务仍在飞，置 `autosave_cleanup_pending`，由任务完成后删除，
    /// 避免在写盘中途删除后又被原子替换重新创建。
    pub(crate) fn cleanup_autosave_on_exit(&self) {
        // 恢复加载尚未完成时关闭：保留文件，避免把正在恢复的唯一副本删掉。
        if self.autosave_recovery.is_some() {
            tracing::info!("正常退出：自动保存恢复尚未完成，保留文件");
            return;
        }
        if self.autosave_in_flight.load(Ordering::SeqCst) {
            self.autosave_cleanup_pending.store(true, Ordering::SeqCst);
            // 备份不参与本次写入，可立即删除；主文件由写入任务完成后删除。
            let backup = autosave_backup_path();
            if backup.exists() {
                let _ = std::fs::remove_file(&backup);
            }
            tracing::info!("正常退出：自动保存仍在写，已安排完成后清理");
            return;
        }

        for path in [autosave_path(), autosave_backup_path()] {
            if !path.exists() {
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => tracing::info!("正常退出，已清理自动保存文件: {:?}", path),
                Err(e) => tracing::warn!("自动保存文件清理失败 {:?}: {e}", path),
            }
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autosave_due_disabled_never_writes() {
        assert!(!autosave_due(false, true, false, false, false));
        assert!(!autosave_due(false, true, true, false, false));
    }

    #[test]
    fn test_autosave_due_clean_project_skips() {
        assert!(!autosave_due(true, false, false, false, false));
    }

    #[test]
    fn test_autosave_due_dirty_project_writes() {
        assert!(autosave_due(true, true, false, false, false));
    }

    #[test]
    fn test_autosave_due_in_flight_skips() {
        assert!(!autosave_due(true, true, true, false, false));
    }

    #[test]
    fn test_autosave_due_manual_or_cloud_save_skips() {
        assert!(!autosave_due(true, true, false, true, false));
        assert!(!autosave_due(true, true, false, false, true));
    }

    #[test]
    fn test_next_autosave_at_interval() {
        let now = Instant::now();
        let next = next_autosave_at(now, 5);
        assert_eq!(
            next.duration_since(now),
            Duration::from_secs(5 * 60),
            "5 分钟间隔应换算为 300 秒"
        );
    }
}
