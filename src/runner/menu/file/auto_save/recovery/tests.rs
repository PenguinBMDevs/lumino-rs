//! 恢复状态机的文件系统用例（DEBT-12）
//!
//! 覆盖卡面验收里必须单测的三项：
//! - 命名冲突（同秒重复改名追加 `-n`，永不覆盖）；
//! - 文件缺失时改名返回 `None`（不伪造路径）；
//! - backup 快照只从已成功载入的文件复制（损坏 main 已被改名，走不到这里）。

use std::fs;
use std::path::PathBuf;

use super::{downgrade_to_unsaved, rename_to_unique_at, sync_backup_snapshot};
use crate::runner::inner::CloudSource;

/// 进程内唯一的临时目录（不依赖 tempfile；测试结束不强制清理，
/// 路径带 pid + tag，重复运行先清空）。
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumino-autosave-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("创建临时目录");
    dir
}

#[test]
fn test_rename_to_unique_keeps_original_content() {
    let dir = temp_dir("rename-basic");
    let path = dir.join("autosave.lmpj");
    fs::write(&path, b"payload").expect("写入候选文件");

    let renamed = rename_to_unique_at(&path, "declined", 42).expect("应改名成功");

    assert!(!path.exists(), "原文件必须已被改名");
    assert_eq!(
        renamed.file_name().and_then(|n| n.to_str()),
        Some("autosave-declined-42.lmpj"),
        "命名格式必须为 <stem>-<tag>-<unix秒>.lmpj"
    );
    assert_eq!(
        fs::read(&renamed).expect("读取改名后文件"),
        b"payload",
        "改名不得改动内容"
    );
}

#[test]
fn test_rename_to_unique_avoids_collision() {
    let dir = temp_dir("rename-collision");
    let path = dir.join("autosave.lmpj");
    fs::write(&path, b"candidate").expect("写入候选文件");
    // 预占同秒的目标名，模拟「同一秒内连续两次改名」
    fs::write(dir.join("autosave-lost-42.lmpj"), b"previous").expect("预占冲突文件名");

    let renamed = rename_to_unique_at(&path, "lost", 42).expect("冲突时应顺延命名");

    assert_eq!(
        renamed.file_name().and_then(|n| n.to_str()),
        Some("autosave-lost-42-1.lmpj"),
        "命名冲突必须追加 -1，绝不覆盖已有文件"
    );
    assert_eq!(
        fs::read(dir.join("autosave-lost-42.lmpj")).expect("读取被保护的文件"),
        b"previous",
        "已有文件内容必须保持不变"
    );
}

#[test]
fn test_rename_to_unique_missing_file_returns_none() {
    let dir = temp_dir("rename-missing");
    let path = dir.join("autosave.lmpj");
    assert!(
        rename_to_unique_at(&path, "lost", 42).is_none(),
        "文件不存在时不得返回伪造路径"
    );
}

#[test]
fn test_sync_backup_snapshot_copies_recovered_content() {
    let dir = temp_dir("backup-sync");
    let from = dir.join("autosave.lmpj");
    let to = dir.join("autosave-backup.lmpj");
    fs::write(&from, b"recovered").expect("写入恢复源");
    fs::write(&to, b"stale").expect("写入旧备份");

    sync_backup_snapshot(from.clone(), &to);

    assert_eq!(
        fs::read(&to).expect("读取备份"),
        b"recovered",
        "恢复成功后 backup 必须来自已成功载入的文件"
    );
    assert!(from.exists(), "复制不得删除源文件");
}

#[test]
fn test_downgrade_to_unsaved_clears_sources() {
    let mut source = Some(PathBuf::from("C:/tmp/autosave.lmpj"));
    let mut cloud = Some(CloudSource {
        conn_id: "conn".to_string(),
        remote_path: "/remote/project.lmpj".to_string(),
    });

    downgrade_to_unsaved(&mut source, &mut cloud);

    assert!(
        source.is_none(),
        "恢复后不得保留磁盘 source（Ctrl+S 必须另存为）"
    );
    assert!(cloud.is_none(), "恢复后不得保留云来源");
}

#[test]
fn test_downgrade_to_unsaved_is_idempotent() {
    let mut source = None;
    let mut cloud = None;
    downgrade_to_unsaved(&mut source, &mut cloud);
    assert!(source.is_none());
    assert!(cloud.is_none());
}
