use serde::{Deserialize, Serialize};

/// 用户界面配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ui: UiConfig,
}

/// 用户界面配置默认值
impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig::default(),
        }
    }
}

/// 用户界面配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
}

/// 用户界面配置默认值
impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "Light".into(),
        }
    }
}
