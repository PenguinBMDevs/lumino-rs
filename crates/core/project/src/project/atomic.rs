//! 原子替换工具（DEBT-01 #118）
//!
//! 保存工程时绝不允许"先截断再写"：一旦写到一半被强杀（看门狗 abort / 断电 /
//! 磁盘满），用户的唯一副本就毁了。这里的统一协议是：
//!
//! 1. 在目标**同卷**创建临时兄弟路径（`.{name}.tmp-{pid}-{seq}`）；
//! 2. 内容完整写入临时路径（单文件写后 `sync_all`；目录内文件尽力 flush）；
//! 3. 换入目标：
//!    - 目标不存在：`rename(tmp, target)`（同卷 rename 原子）；
//!    - 目标存在：`rename(target, bak)` → `rename(tmp, target)`；
//!      第二步失败时回滚 `rename(bak, target)`；成功后尽力删除 `.bak`。
//!
//! 任何一步失败都不会留下"半截的目标文件"；最坏情形是目标缺失而 `.bak`
//! 完好，错误信息会带上备份路径，用户可手动恢复。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lumino_core::error::{CoreError, Result};

/// 同卷临时路径的进程内序号：避免同一进程并发保存时互踩临时文件。
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// 目标的同卷临时兄弟路径。
fn sibling_tmp(target: &Path) -> Result<PathBuf> {
    let file_name = target.file_name().ok_or_else(|| {
        CoreError::InvalidArgument(format!("无效的保存路径: {}", target.display()))
    })?;
    let parent = parent_dir(target);
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    Ok(parent.join(format!(
        ".{}.tmp-{}-{}",
        file_name.to_string_lossy(),
        std::process::id(),
        seq
    )))
}

/// 目标的备份路径（`{name}.bak`，与目标同目录）。
fn backup_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    parent_dir(target).join(name)
}

fn parent_dir(target: &Path) -> PathBuf {
    match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// 删除文件或目录（权限不足等错误原样返回）。
fn remove_path(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// 把已写好的临时内容换入目标（两步 rename + 回滚）。
fn swap_into_place(tmp: &Path, target: &Path) -> Result<()> {
    if !target.exists() {
        std::fs::rename(tmp, target)?;
        return Ok(());
    }

    // 只读目标是用户显式的保护意图：拒绝覆盖，不做"rename 绕过只读位"的小动作。
    if std::fs::metadata(target)
        .map(|m| m.permissions().readonly())
        .unwrap_or(false)
    {
        return Err(CoreError::Other(format!(
            "保存目标为只读，无法覆盖：{}（请先取消只读属性）",
            target.display()
        )));
    }

    let bak = backup_path(target);
    if bak.exists() {
        remove_path(&bak).map_err(|e| {
            CoreError::Io(std::io::Error::new(
                e.kind(),
                format!("无法清理旧备份 {}：{e}", bak.display()),
            ))
        })?;
    }

    std::fs::rename(target, &bak).map_err(|e| {
        CoreError::Io(std::io::Error::new(
            e.kind(),
            format!("无法备份原文件到 {}：{e}", bak.display()),
        ))
    })?;

    if let Err(e) = std::fs::rename(tmp, target) {
        // 换入失败：把备份放回去，保证目标回到旧内容
        if let Err(rb) = std::fs::rename(&bak, target) {
            return Err(CoreError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "换入 {} 失败：{e}；回滚失败：{rb}（原文件保留在 {}）",
                    target.display(),
                    bak.display()
                ),
            )));
        }
        return Err(CoreError::Io(e));
    }

    // 成功后清理备份；失败仅遗留 .bak，不影响工程正确性
    let _ = remove_path(&bak);
    Ok(())
}

/// 单文件原子保存：`write` 把内容写到给定的临时路径。
///
/// 失败注入单测直接传入返回错误的 `write`，验证原文件不受影响。
pub(crate) fn save_file_atomic<F>(target: &Path, write: F) -> Result<()>
where
    F: FnOnce(&Path) -> Result<()>,
{
    let tmp = sibling_tmp(target)?;
    if let Err(e) = write(&tmp) {
        let _ = remove_path(&tmp);
        return Err(e);
    }
    if let Err(e) = swap_into_place(&tmp, target) {
        let _ = remove_path(&tmp);
        return Err(e);
    }
    Ok(())
}

/// 便捷入口：把 `bytes` 原子写入 `target`（写临时文件 → sync_all → 换入）。
pub(crate) fn write_file_atomic(target: &Path, bytes: &[u8]) -> Result<()> {
    save_file_atomic(target, |tmp| {
        let mut file = std::fs::File::create(tmp)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
        Ok(())
    })
}

/// 目录原子保存：`write` 在给定的临时目录内完整构建新内容。
pub(crate) fn save_dir_atomic<F>(target: &Path, write: F) -> Result<()>
where
    F: FnOnce(&Path) -> Result<()>,
{
    let tmp = sibling_tmp(target)?;
    if tmp.exists() {
        let _ = remove_path(&tmp);
    }
    std::fs::create_dir_all(&tmp)?;

    if let Err(e) = write(&tmp) {
        let _ = remove_path(&tmp);
        return Err(e);
    }

    // 尽力 flush（进程被强杀不需要 fsync；断电场景靠这一层兜底）
    sync_dir_best_effort(&tmp);

    if let Err(e) = swap_into_place(&tmp, target) {
        let _ = remove_path(&tmp);
        return Err(e);
    }
    Ok(())
}

/// 递归 flush 目录内文件：尽力而为，失败不阻断保存。
fn sync_dir_best_effort(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sync_dir_best_effort(&path);
            continue;
        }
        if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&path) {
            let _ = file.sync_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lumino_atomic_{name}_{}_{}",
            std::process::id(),
            TMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("创建测试目录失败");
        dir
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .expect("读取测试目录失败")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-") || n.ends_with(".bak"))
            .collect()
    }

    #[test]
    fn write_file_atomic_replaces_existing_content() {
        let dir = test_dir("replace_file");
        let target = dir.join("project.lmpj");
        std::fs::write(&target, b"old-content").expect("写入旧内容失败");

        write_file_atomic(&target, b"new-content").expect("原子写入失败");

        assert_eq!(
            std::fs::read(&target).expect("读取结果失败"),
            b"new-content"
        );
        assert!(
            leftovers(&dir).is_empty(),
            "不应残留 tmp/bak：{:?}",
            leftovers(&dir)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_file_atomic_creates_missing_target() {
        let dir = test_dir("create_file");
        let target = dir.join("new.lmpj");

        write_file_atomic(&target, b"content").expect("原子写入失败");

        assert_eq!(std::fs::read(&target).expect("读取结果失败"), b"content");
        assert!(leftovers(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_file_atomic_keeps_original_on_write_failure() {
        let dir = test_dir("fail_file");
        let target = dir.join("project.lmpj");
        std::fs::write(&target, b"precious").expect("写入旧内容失败");

        let result = save_file_atomic(&target, |tmp| {
            // 模拟"写到一半磁盘满/被中断"：先写入部分内容再失败
            std::fs::write(tmp, b"partial").expect("写临时文件失败");
            Err(CoreError::Other("磁盘已满（注入）".into()))
        });

        assert!(result.is_err(), "注入失败必须向外传播");
        assert_eq!(
            std::fs::read(&target).expect("读取旧内容失败"),
            b"precious",
            "保存失败后原文件必须保持不变"
        );
        assert!(leftovers(&dir).is_empty(), "失败路径不应残留 tmp/bak");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_file_atomic_failure_does_not_create_target() {
        let dir = test_dir("fail_absent");
        let target = dir.join("never.lmpj");

        let result = save_file_atomic(&target, |tmp| {
            std::fs::write(tmp, b"partial").expect("写临时文件失败");
            Err(CoreError::Other("磁盘已满（注入）".into()))
        });

        assert!(result.is_err());
        assert!(!target.exists(), "失败时不得创建半截目标文件");
        assert!(leftovers(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_dir_atomic_replaces_whole_directory() {
        let dir = test_dir("replace_dir");
        let target = dir.join("project.lmpj");
        std::fs::create_dir_all(target.join("data")).expect("创建旧目录失败");
        std::fs::write(target.join("data/old.bin"), b"old").expect("写入旧文件失败");

        save_dir_atomic(&target, |tmp| {
            std::fs::create_dir_all(tmp.join("data"))?;
            std::fs::write(tmp.join("data/new.bin"), b"new")?;
            Ok(())
        })
        .expect("原子目录保存失败");

        assert!(target.join("data/new.bin").exists(), "新内容必须换入");
        assert!(!target.join("data/old.bin").exists(), "旧内容必须整体消失");
        assert!(
            leftovers(&dir).is_empty(),
            "不应残留 tmp/bak：{:?}",
            leftovers(&dir)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_dir_atomic_keeps_original_on_failure() {
        let dir = test_dir("fail_dir");
        let target = dir.join("project.lmpj");
        std::fs::create_dir_all(target.join("data")).expect("创建旧目录失败");
        std::fs::write(target.join("data/old.bin"), b"precious").expect("写入旧文件失败");

        let result = save_dir_atomic(&target, |tmp| {
            std::fs::create_dir_all(tmp.join("data"))?;
            std::fs::write(tmp.join("data/partial.bin"), b"partial")?;
            Err(CoreError::Other("写入中断（注入）".into()))
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read(target.join("data/old.bin")).expect("读取旧文件失败"),
            b"precious",
            "失败后旧工程必须原样保留"
        );
        assert!(!target.join("data/partial.bin").exists());
        assert!(leftovers(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 只读目标是用户显式的保护意图：必须拒绝覆盖且原内容不变。
    #[test]
    fn write_file_atomic_refuses_readonly_target() {
        let dir = test_dir("readonly_file");
        let target = dir.join("project.lmpj");
        std::fs::write(&target, b"protected").expect("写入失败");

        let mut perms = std::fs::metadata(&target)
            .expect("读取权限失败")
            .permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&target, perms).expect("设置只读失败");

        let result = write_file_atomic(&target, b"new");

        // 先恢复可写，保证测试目录能清理干净
        let mut perms = std::fs::metadata(&target)
            .expect("读取权限失败")
            .permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&target, perms).expect("恢复可写失败");

        assert!(result.is_err(), "只读目标必须拒绝覆盖");
        assert_eq!(
            std::fs::read(&target).expect("读取原内容失败"),
            b"protected",
            "拒绝覆盖后原内容必须不变"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
