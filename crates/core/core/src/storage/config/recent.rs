//! 最近打开的文件列表（文件菜单「最近打开」子菜单的数据源）。
//!
//! 与撤销历史无关：这里只记录**用户成功打开/保存过的文件路径**，供菜单快速重开。
//! 列表最新在前、按路径去重、超出上限截尾；持久化在 `UiConfig::recent_files`。

use std::path::Path;

use serde::{Deserialize, Serialize};

/// 「最近打开」最多保留的条目数。
///
/// 子菜单展示，10 条的高度约 `10 × 28 + 清除项 + 分隔线 + 内边距 ≈ 325px`，
/// 在 720p 屏幕上不会溢出。
pub const RECENT_FILES_LIMIT: usize = 10;

/// 最近打开/保存的文件条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentFile {
    /// 用户最初选择/保存的原始路径。
    ///
    /// 压缩包记录压缩包本身，**不是**解压后的临时文件；目录工程记录目录路径。
    pub path: String,
    /// 最近一次成功打开/保存的 Unix 秒（`0` = 旧数据或系统时钟异常）。
    ///
    /// 当前只用于未来展示/清理策略，排序以列表顺序为准。
    #[serde(default)]
    pub opened_at: u64,
}

/// 记录一个最近文件：去重后置顶，超过 `limit` 截尾。
///
/// 传空路径是空操作（防御调用方拿到无效路径）。
pub fn remember(list: &mut Vec<RecentFile>, path: &Path, now: u64, limit: usize) {
    let path = path.to_string_lossy();
    if path.is_empty() {
        return;
    }
    list.retain(|entry| !same_path(&entry.path, path.as_ref()));
    list.insert(
        0,
        RecentFile {
            path: path.into_owned(),
            opened_at: now,
        },
    );
    list.truncate(limit);
}

/// 从最近文件列表移除指定路径（文件已不存在时自动清理）。
pub fn forget(list: &mut Vec<RecentFile>, path: &Path) {
    let path = path.to_string_lossy();
    list.retain(|entry| !same_path(&entry.path, path.as_ref()));
}

/// 路径比较：Windows 文件系统大小写不敏感，其余平台按字节比较。
fn same_path(a: &str, b: &str) -> bool {
    #[cfg(windows)]
    {
        a.eq_ignore_ascii_case(b)
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[RecentFile]) -> Vec<&str> {
        list.iter().map(|entry| entry.path.as_str()).collect()
    }

    #[test]
    fn test_new_path_goes_to_front() {
        let mut list = Vec::new();
        remember(&mut list, Path::new("a.mid"), 100, RECENT_FILES_LIMIT);
        remember(&mut list, Path::new("b.mid"), 200, RECENT_FILES_LIMIT);
        assert_eq!(paths(&list), vec!["b.mid", "a.mid"]);
        assert_eq!(list[0].opened_at, 200);
    }

    #[test]
    fn test_duplicate_moves_to_front_without_duplication() {
        let mut list = Vec::new();
        remember(&mut list, Path::new("a.mid"), 100, RECENT_FILES_LIMIT);
        remember(&mut list, Path::new("b.mid"), 200, RECENT_FILES_LIMIT);
        remember(&mut list, Path::new("a.mid"), 300, RECENT_FILES_LIMIT);
        assert_eq!(paths(&list), vec!["a.mid", "b.mid"]);
        assert_eq!(list[0].opened_at, 300, "重复项应刷新时间戳");
    }

    #[test]
    fn test_limit_truncates_oldest() {
        let mut list = Vec::new();
        for index in 0..5 {
            remember(
                &mut list,
                Path::new(&format!("{index}.mid")),
                index as u64,
                3,
            );
        }
        assert_eq!(paths(&list), vec!["4.mid", "3.mid", "2.mid"]);
    }

    #[test]
    fn test_forget_removes_only_target() {
        let mut list = Vec::new();
        remember(&mut list, Path::new("a.mid"), 1, RECENT_FILES_LIMIT);
        remember(&mut list, Path::new("b.mid"), 2, RECENT_FILES_LIMIT);
        forget(&mut list, Path::new("a.mid"));
        assert_eq!(paths(&list), vec!["b.mid"]);
        forget(&mut list, Path::new("missing.mid"));
        assert_eq!(paths(&list), vec!["b.mid"], "不存在项为空操作");
    }

    #[test]
    fn test_empty_path_is_ignored() {
        let mut list = Vec::new();
        remember(&mut list, Path::new(""), 1, RECENT_FILES_LIMIT);
        assert!(list.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_path_case_is_deduplicated() {
        let mut list = Vec::new();
        remember(
            &mut list,
            Path::new(r"C:\Music\Song.mid"),
            1,
            RECENT_FILES_LIMIT,
        );
        remember(
            &mut list,
            Path::new(r"c:\music\song.mid"),
            2,
            RECENT_FILES_LIMIT,
        );
        assert_eq!(list.len(), 1, "Windows 路径大小写不同视为同一文件");
        assert_eq!(list[0].opened_at, 2);
    }

    #[cfg(not(windows))]
    #[test]
    fn test_unix_path_case_is_case_sensitive() {
        let mut list = Vec::new();
        remember(
            &mut list,
            Path::new("/music/Song.mid"),
            1,
            RECENT_FILES_LIMIT,
        );
        remember(
            &mut list,
            Path::new("/music/song.mid"),
            2,
            RECENT_FILES_LIMIT,
        );
        assert_eq!(list.len(), 2, "Unix 大小写敏感，视为两个文件");
    }
}
