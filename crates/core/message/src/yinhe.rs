//! Yinhe 副模式动作

use serde::{Deserialize, Serialize};

/// Yinhe 视图模式（对齐 yinhe `ViewMode`：Arrange/Mix/Edit→Piano）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum YinheViewMode {
    Arrange,
    Piano,
    Mix,
}

impl Default for YinheViewMode {
    fn default() -> Self {
        Self::Arrange
    }
}

impl YinheViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Arrange => "ARRANGE",
            Self::Piano => "PIANO",
            Self::Mix => "MIX",
        }
    }
}

/// Yinhe 副模式动作
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum YinheAction {
    /// 切换视图模式
    ViewModeChanged(YinheViewMode),
    /// 切换 PianoRoll 在 Arrange 中的叠加显示
    TogglePianorollInArrange,
}
