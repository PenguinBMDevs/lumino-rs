//! 缓存相关的工具函数

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// 计算缓存键
///
/// 基于文件路径和修改时间生成唯一的缓存键
pub fn compute_cache_key(path: &Path, file_modified: std::time::SystemTime) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    file_modified.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_compute_cache_key_consistency() {
        let path = Path::new("/test/path/file.txt");
        let time = SystemTime::UNIX_EPOCH;

        let key1 = compute_cache_key(path, time);
        let key2 = compute_cache_key(path, time);

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_compute_cache_key_different_paths() {
        let time = SystemTime::UNIX_EPOCH;
        let path1 = Path::new("/test/path/file1.txt");
        let path2 = Path::new("/test/path/file2.txt");

        let key1 = compute_cache_key(path1, time);
        let key2 = compute_cache_key(path2, time);

        assert_ne!(key1, key2);
    }
}
