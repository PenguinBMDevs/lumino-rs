//! 崩溃日志扫描/归档测试。

use super::*;

fn temp_log_dir() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("lumino-log-test-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&dir).expect("应能创建临时日志目录");
    dir
}

#[test]
fn test_pending_crash_logs_filters_and_archives() {
    let dir = temp_log_dir();
    fs::write(dir.join("crash_20260101_000000.log"), "panic").expect("写崩溃日志");
    fs::write(dir.join("crash_20260102_000000.log"), "panic").expect("写崩溃日志");
    fs::write(dir.join("lumino_20260101_000000.log"), "normal").expect("写普通日志");

    let pending = pending_crash_logs(&dir);
    assert_eq!(pending.len(), 2, "只扫描 crash_*.log");
    assert_eq!(
        pending[0].file_name().and_then(|n| n.to_str()),
        Some("crash_20260101_000000.log"),
        "按文件名升序"
    );

    let archive = archive_crash_logs(&dir, &pending).expect("归档应成功");
    assert!(archive.exists(), "归档目录应存在");
    assert!(
        pending_crash_logs(&dir).is_empty(),
        "归档后根目录不应再有待提示崩溃日志"
    );
    assert!(
        dir.join("lumino_20260101_000000.log").exists(),
        "普通日志不得被归档"
    );

    fs::remove_dir_all(&dir).expect("清理临时日志目录");
}

#[test]
fn test_archive_crash_logs_avoids_name_collision() {
    let dir = temp_log_dir();
    let source = dir.join("crash_same.log");
    fs::write(&source, "panic").expect("写崩溃日志");
    let archive_dir = dir.join("crash-archive");
    fs::create_dir_all(&archive_dir).expect("创建归档目录");
    fs::write(archive_dir.join("crash_same.log"), "old").expect("写旧归档");

    archive_crash_logs(&dir, &[source]).expect("归档应成功");
    assert!(
        archive_dir.join("crash_same-1.log").exists(),
        "同名归档必须追加序号，不得覆盖"
    );

    fs::remove_dir_all(&dir).expect("清理临时日志目录");
}
