//! 测试文件命名棘轮（DEBT-10 / T-40）。
//!
//! 仓库历史上存在多套测试命名：`tests.rs`、`*_tests.rs`、`tests_*.rs`、
//! `tests/` 子目录。T-40b 已把 90 个存量文件全部迁入 `tests.rs` / `tests/`，
//! 白名单已清零；此后新增不合规文件会直接失败。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "target" | "docs" | "node_modules" | ".git") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// 旧命名：`foo_tests.rs` / `tests_foo.rs`（`tests.rs` 本身合法）。
fn is_legacy_test_name(name: &str) -> bool {
    if name == "tests.rs" {
        return false;
    }
    name.ends_with("_tests.rs") || name.starts_with("tests_")
}

#[test]
fn test_naming_is_ratcheted() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let whitelist_path = root.join("tests").join("test_naming_whitelist.txt");
    let whitelist: BTreeSet<String> = std::fs::read_to_string(&whitelist_path)
        .expect("读取测试命名白名单")
        .lines()
        .map(|line| line.trim().replace('\\', "/"))
        .filter(|line| !line.is_empty())
        .collect();

    let mut files = Vec::new();
    for dir in ["crates", "src", "tests"] {
        rust_files(&root.join(dir), &mut files);
    }

    let mut offenders = Vec::new();
    for file in files {
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !is_legacy_test_name(name) {
            continue;
        }
        let rel = file
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if !whitelist.contains(&rel) {
            offenders.push(rel);
        }
    }

    offenders.sort();
    assert!(
        offenders.is_empty(),
        "新增测试文件请使用 `tests.rs` 或 `tests/` 子目录；以下文件未登记在白名单：{offenders:?}"
    );
}
