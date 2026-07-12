use lumino_core::storage::config::*;
use std::{fs, io, path::PathBuf};

#[derive(Debug)]
pub struct ConfigWrapper {
    inner: Config,
    path: PathBuf,
    dirty: bool,
}

impl ConfigWrapper {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let inner = Self::load_or_migrate(&path)?;
        Ok(Self {
            inner,
            path,
            dirty: false,
        })
    }

    /// 加载配置文件，支持从旧版 TOML 格式迁移到 JSON
    fn load_or_migrate(path: &PathBuf) -> io::Result<Config> {
        // 优先读取 JSON 格式
        if path.exists() {
            let bytes = fs::read(path)?;
            match serde_json::from_slice(&bytes) {
                Ok(config) => return Ok(config),
                Err(e) => {
                    tracing::warn!("JSON 配置文件解析失败 ({}), 使用默认配置", e);
                    return Ok(Config::default());
                }
            }
        }

        // JSON 不存在时，尝试从旧版 TOML 迁移
        let toml_path = path.with_extension("toml");
        if toml_path.exists() {
            tracing::info!("检测到旧版 TOML 配置文件，迁移到 JSON 格式");
            let bytes = fs::read(&toml_path)?;
            match toml::from_slice::<Config>(&bytes) {
                Ok(config) => {
                    // 迁移成功，写入 JSON 格式
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    let json_str = serde_json::to_string_pretty(&config)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    fs::write(path, json_str.as_bytes())?;
                    // 删除旧版 TOML 文件
                    if let Err(e) = fs::remove_file(&toml_path) {
                        tracing::warn!("删除旧版 TOML 配置文件失败: {}", e);
                    }
                    tracing::info!("配置文件迁移完成: TOML -> JSON");
                    return Ok(config);
                }
                Err(e) => {
                    tracing::warn!("旧版 TOML 配置文件解析失败 ({}), 使用默认配置", e);
                    return Ok(Config::default());
                }
            }
        }

        // 都不存在，使用默认配置
        Ok(Config::default())
    }

    pub fn get(&self) -> &Config {
        &self.inner
    }
    pub fn patch<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Config),
    {
        f(&mut self.inner);
        self.dirty = true;
    }
    pub fn save(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let str = serde_json::to_string_pretty(&self.inner)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&self.path, str.as_bytes())?;
        self.dirty = false;
        Ok(())
    }
}
