mod config;
mod ui_state;

use std::io;

use directories::ProjectDirs;

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "PenguinBMDevs";
const APPLICATION: &str = "lumino";

#[derive(Debug)]
pub struct Storage {
    pub config: config::ConfigWrapper,
    pub ui_state: ui_state::UiStateWrapper,
}

// 存储系统，存一些配置文件和状态文件
impl Storage {
    // 创建一个新的存储系统
    pub fn new() -> io::Result<Self> {
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
