use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiState {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub w: u32,
    pub h: u32,
    pub is_maximized: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            w: 1440,
            h: 900,
            is_maximized: false,
        }
    }
}
