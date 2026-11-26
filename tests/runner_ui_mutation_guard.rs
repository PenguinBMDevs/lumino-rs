//! Runner 不得直接改 UI 状态的回归守卫（DEBT-10 / T-23）。
//!
//! Runner 统一走 `Root::push_toast` / `Root::mark_project_modified` 等命名入口；
//! 这里扫描 `src/runner`，防止有人再把 `root_mut().toast.push(...)` 或
//! `editor_state.data.modified = true` 写回去。

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("读取 src/runner 目录");
    for entry in entries {
        let path = entry.expect("读取目录项").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn runner_has_no_direct_toast_or_modified_writes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("runner");
    let mut files = Vec::new();
    rust_files(&root, &mut files);

    let mut offenders = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("读取 runner 源文件");
        if text.contains(".toast.push(")
            || text.contains("editor_state.data.modified = true")
            || text.contains("root_mut().editor")
            || text.contains("root_mut()\n            .editor")
        {
            offenders.push(file.display().to_string());
        }
    }

    assert!(
        offenders.is_empty(),
        "Runner 不得直接改 UI 状态，请走 Root 命令入口：{offenders:?}"
    );
}
