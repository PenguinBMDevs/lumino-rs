//! Runner 文件菜单：文件元数据工具

/// 从文件路径读取文件创建时间并格式化为本地时间字符串
pub(super) fn format_created_at_from_path(path: &std::path::Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let created = metadata.modified().ok()?;
    let since_epoch = created
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs() as i64;
    let datetime = chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.with_timezone(&chrono::Local))
        .unwrap_or_else(chrono::Local::now);
    Some(datetime.format("%Y-%m-%d %H:%M:%S").to_string())
}
