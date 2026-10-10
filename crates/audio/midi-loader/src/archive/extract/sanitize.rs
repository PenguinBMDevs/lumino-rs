//! 归档条目名安全拼接（DEBT-02 #119 / CWE-22 Zip Slip）
//!
//! 归档（ZIP/RAR/7z/ISO…）的条目名是**不可信输入**：`../`、绝对路径、
//! Windows 盘符 / UNC / NTFS 备用数据流（ADS）都可能把文件写到目标目录之外。
//! 所有「条目名 → 输出路径」的拼接必须走 [`safe_join`]，不得直接 `join`。

use std::path::{Path, PathBuf};

use crate::archive::ArchiveError;

/// 把归档条目名安全地拼到输出目录下。
///
/// 规则（任一违反即返回 [`ArchiveError::UnsafeEntryName`]）：
/// - 空名 / 全部分隔符；
/// - 绝对路径（以 `/` 或 `\` 开头，含 UNC）；
/// - 含 `..` 组件；
/// - 组件含 `:`（Windows 盘符 / NTFS ADS / 设备名保护）；
/// - 反斜杠统一视为分隔符（压缩包跨平台常见）。
///
/// `.` 与空组件（连续分隔符）直接忽略。
pub(super) fn safe_join(output_dir: &Path, name: &str) -> Result<PathBuf, ArchiveError> {
    let normalized = name.replace('\\', "/");
    if normalized.is_empty() || normalized.chars().all(|c| c == '/') {
        return Err(ArchiveError::UnsafeEntryName(name.to_string()));
    }
    if normalized.starts_with('/') {
        return Err(ArchiveError::UnsafeEntryName(name.to_string()));
    }

    let mut out = output_dir.to_path_buf();
    let mut pushed = 0usize;
    for comp in normalized.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp == ".." || comp.contains(':') {
            return Err(ArchiveError::UnsafeEntryName(name.to_string()));
        }
        out.push(comp);
        pushed += 1;
    }
    if pushed == 0 {
        return Err(ArchiveError::UnsafeEntryName(name.to_string()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> &'static Path {
        Path::new("out")
    }

    #[test]
    fn accepts_normal_relative_names() {
        assert_eq!(
            safe_join(base(), "a.mid").expect("合法"),
            Path::new("out/a.mid")
        );
        assert_eq!(
            safe_join(base(), "sub/a.mid").expect("合法"),
            Path::new("out/sub/a.mid")
        );
        // 反斜杠视为分隔符（跨平台压缩包）
        assert_eq!(
            safe_join(base(), "sub\\a.mid").expect("合法"),
            Path::new("out/sub/a.mid")
        );
        assert_eq!(
            safe_join(base(), "./a.mid").expect("合法"),
            Path::new("out/a.mid")
        );
        assert_eq!(
            safe_join(base(), "sub//a.mid").expect("合法"),
            Path::new("out/sub/a.mid")
        );
    }

    #[test]
    fn rejects_traversal_absolute_and_drive_prefixes() {
        let bad = [
            "../evil.mid",
            "sub/../../evil.mid",
            "..\\evil.mid",
            "/etc/passwd",
            "\\windows\\evil",
            "C:\\evil.mid",
            "c:/evil.mid",
            "sub/C:evil.mid",
            "sub/..",
            "../",
            "..",
            "",
            "/",
            "//server/share/x",
        ];
        for name in bad {
            assert!(
                safe_join(base(), name).is_err(),
                "必须拒绝不安全条目名: {name:?}"
            );
        }
    }
}
