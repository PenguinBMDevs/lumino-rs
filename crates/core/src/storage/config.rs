use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ui: UiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,

}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "Light".into(),
        }
    }
}
