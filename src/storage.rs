mod config;
mod ui_state;

use std::io;

use directories::ProjectDirs;

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "buickmeow";
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
        // 创建项目目录
        let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
            .expect("Create ProjectDirs"); // 确保项目目录存在

        // 创建配置文件目录
        let config = dirs.config_dir().to_owned();
        // 创建偏好设置目录
        let preference = dirs.preference_dir().to_owned();

        Ok(Self { // 成功
            config: config::ConfigWrapper::new(config.join("config.toml"))?,
            ui_state: ui_state::UiStateWrapper::new(preference.join("ui_state.json")),
        })
    }
}
