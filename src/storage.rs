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

impl Storage {
    pub fn new() -> io::Result<Self> {
        let dirs =
            ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION).expect("Create ProjectDirs");

        let config = dirs.config_dir().to_owned();
        let preference = dirs.preference_dir().to_owned();

        Ok(Self {
            config: config::ConfigWrapper::new(config.join("config.toml"))?,
            ui_state: ui_state::UiStateWrapper::new(preference.join("ui_state.json")),
        })
    }
}
