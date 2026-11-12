//! 自动保存恢复判定：纯函数状态机（DEBT-12）
//!
//! 从 `recovery.rs` 抽出「文件状态 → 下一步动作」的全部判定，使恢复语义可以
//! 脱离 winit / 文件系统单测。I/O（改名、复制、加载）仍留在调用方，但**分支**
//! 只由这里的函数决定。
//!
//! 覆盖的六种状态（卡面验收）：
//! 1. main 有效 → 用 main；
//! 2. main 存在但损坏 → 改名 lost，尝试 backup；
//! 3. main 缺失、backup 有效 → 直接用 backup；
//! 4. 双损坏 → 保留两份 lost，提示一次后结束；
//! 5. 用户拒绝 → 两份都改名 declined；
//! 6. 命名冲突 → `rename_to_unique` 追加 `-n`（在 `recovery/tests.rs` 覆盖）。

use std::path::PathBuf;

/// 恢复候选来源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoverySource {
    /// 主自动保存文件
    Main,
    /// 备份文件
    Backup,
}

impl RecoverySource {
    /// 该来源对应的候选路径
    pub(crate) fn path(self, main: &std::path::Path, backup: &std::path::Path) -> PathBuf {
        match self {
            Self::Main => main.to_path_buf(),
            Self::Backup => backup.to_path_buf(),
        }
    }

    /// 是否使用备份（写入 `AutosaveRecovery`）
    pub(crate) fn is_backup(self) -> bool {
        matches!(self, Self::Backup)
    }
}

/// 启动时候选：main 优先，其次 backup；都没有则不询问。
pub(crate) fn startup_candidate(main_exists: bool, backup_exists: bool) -> Option<RecoverySource> {
    if main_exists {
        Some(RecoverySource::Main)
    } else if backup_exists {
        Some(RecoverySource::Backup)
    } else {
        None
    }
}

/// 加载失败后的下一步
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadFailureNext {
    /// 当前是 main 且 backup 存在 → 改名 lost 后尝试 backup
    FallbackToBackup,
    /// 当前已是 backup，或没有 backup → 报告丢失
    ReportLost,
}

/// 解析失败后的分支：只有「主文件失败 + 备份存在」才回退备份。
pub(crate) fn after_load_failed(using_backup: bool, backup_exists: bool) -> LoadFailureNext {
    if !using_backup && backup_exists {
        LoadFailureNext::FallbackToBackup
    } else {
        LoadFailureNext::ReportLost
    }
}

/// 恢复成功后 backup 同步方向：(源, 目标)。
///
/// - 正常恢复（main）：main → backup（把上次启动快照固化为备份）；
/// - 备份回退（backup）：backup → main（重建主槽位）。
pub(crate) fn backup_sync_direction(using_backup: bool) -> (RecoverySource, RecoverySource) {
    if using_backup {
        (RecoverySource::Backup, RecoverySource::Main)
    } else {
        (RecoverySource::Main, RecoverySource::Backup)
    }
}

/// 是否允许把 main 轮流复制为 backup。
///
/// **红线**：损坏的 main 绝不能覆盖有效 backup。只有本会话已经确认可信的
/// main（本次写入成功 / 刚成功恢复）才允许轮换。
pub(crate) fn may_rotate_backup(main_trusted: bool, main_exists: bool) -> bool {
    main_trusted && main_exists
}

/// 用户拒绝恢复时的改名标签（两份文件都保留）。
pub(crate) const DECLINED_TAG: &str = "declined";
/// 解析失败时的改名标签（永不删除损坏文件）。
pub(crate) const LOST_TAG: &str = "lost";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_startup_candidate_prefers_main() {
        assert_eq!(
            startup_candidate(true, true),
            Some(RecoverySource::Main),
            "主文件存在时必须优先主文件"
        );
        assert_eq!(
            startup_candidate(false, true),
            Some(RecoverySource::Backup),
            "主文件缺失时回退备份"
        );
        assert_eq!(
            startup_candidate(false, false),
            None,
            "两份都不存在则不询问"
        );
    }

    #[test]
    fn test_after_load_failed_falls_back_only_from_main() {
        assert_eq!(
            after_load_failed(false, true),
            LoadFailureNext::FallbackToBackup,
            "主文件损坏且有备份 → 回退备份"
        );
        assert_eq!(
            after_load_failed(false, false),
            LoadFailureNext::ReportLost,
            "主文件损坏且无备份 → 报告丢失"
        );
        assert_eq!(
            after_load_failed(true, true),
            LoadFailureNext::ReportLost,
            "备份也失败 → 不得再次回退（防死循环）"
        );
    }

    #[test]
    fn test_backup_sync_direction() {
        assert_eq!(
            backup_sync_direction(false),
            (RecoverySource::Main, RecoverySource::Backup),
            "正常恢复：main → backup"
        );
        assert_eq!(
            backup_sync_direction(true),
            (RecoverySource::Backup, RecoverySource::Main),
            "备份回退：backup → main"
        );
    }

    #[test]
    fn test_may_rotate_backup_rejects_untrusted_main() {
        assert!(may_rotate_backup(true, true), "可信 main 允许轮换");
        assert!(
            !may_rotate_backup(false, true),
            "不可信（损坏）main 绝不能覆盖有效 backup"
        );
        assert!(!may_rotate_backup(true, false), "main 不存在时无可轮换");
    }
}
