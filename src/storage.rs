mod config;
mod ui_state;

use std::{fs, io};

use directories::ProjectDirs;

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "PenguinBMDevs";
const APPLICATION: &str = "lumino";

/// 旧组织名（buickmeow），用于迁移旧版配置文件
const OLD_ORGANIZATION: &str = "buickmeow";

#[derive(Debug)]
pub struct Storage {
    pub config: config::ConfigWrapper,
    pub ui_state: ui_state::UiStateWrapper,
}

/// 将整个配置目录从旧路径迁移到新路径
///
/// 检查 `com.buickmeow.lumino` 是否存在且 `com.PenguinBMDevs.lumino` 尚不存在，
/// 若成立则完整复制所有旧配置文件到新路径，然后删除旧目录。
fn migrate_old_dir() {
    let old_dirs = match ProjectDirs::from(QUALIFIER, OLD_ORGANIZATION, APPLICATION) {
        Some(d) => d,
        None => return,
    };
    let new_dirs = match ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION) {
        Some(d) => d,
        None => return,
    };

    let old_config = old_dirs.config_dir();
    let new_config = new_dirs.config_dir();
    let old_pref = old_dirs.preference_dir();
    let new_pref = new_dirs.preference_dir();

    // 旧目录不存在 → 无需迁移
    if !old_config.exists() && !old_pref.exists() {
        return;
    }
    // 新目录已存在 → 用户可能已经手动创建或之前迁移过，跳过
    if new_config.exists() {
        tracing::debug!("新配置目录已存在，跳过迁移");
        return;
    }

    tracing::info!(
        "检测到旧版配置目录 ({:?}), 迁移到新目录 ({:?})",
        old_config,
        new_config
    );

    // 迁移 config 目录
    if old_config.exists() {
        if let Err(e) = copy_dir(old_config, new_config) {
            tracing::warn!("配置文件迁移失败: {}", e);
        } else {
            tracing::info!("配置文件目录迁移完成, 清理旧目录");
            let _ = fs::remove_dir_all(old_config);
        }
    }

    // 迁移 preference 目录（可能包含 ui_state.json）
    if old_pref.exists() {
        if let Err(e) = copy_dir(old_pref, new_pref) {
            tracing::warn!("偏好设置文件迁移失败: {}", e);
        } else {
            let _ = fs::remove_dir_all(old_pref);
        }
    }
}

/// 递归复制目录内容
fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if file_type.is_dir() {
            copy_dir(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// 存储系统，存一些配置文件和状态文件
impl Storage {
    // 创建一个新的存储系统
    pub fn new() -> io::Result<Self> {
        // 先尝试从旧版目录迁移配置文件
        migrate_old_dir();

        let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法创建项目目录"))?;

        // 创建配置文件目录
        let config = dirs.config_dir().to_owned();
        // 创建偏好设置目录
        let preference = dirs.preference_dir().to_owned();

        Ok(Self {
            // 成功
            config: config::ConfigWrapper::new(config.join("config.json"))?,
            ui_state: ui_state::UiStateWrapper::new(preference.join("ui_state.json")),
        })
    }
}
