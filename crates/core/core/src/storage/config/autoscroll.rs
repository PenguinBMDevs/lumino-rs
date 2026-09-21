use serde::{Deserialize, Serialize};

/// 自动滚动模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AutoScrollMode {
    /// 模式1：固定指示线到左侧，卷帘自动左移
    FixedIndicatorLeft,
    /// 模式2：指示线移动，到右侧翻页
    #[default]
    ScrollingIndicator,
    /// 关闭自动滚动
    Off,
}

/// 自动滚动配置
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutoScrollConfig {
    /// 当前自动滚动模式
    pub mode: AutoScrollMode,
    /// 模式1：指示线固定位置（从左边缘算起，像素）
    pub fixed_indicator_position: u32,
    /// 模式2：翻页触发位置（从右边缘算起，像素）
    pub page_trigger_offset: u32,
    /// 模式2：翻页后指示线回到的位置（从左边缘算起，像素）
    pub page_return_position: u32,
}

impl Default for AutoScrollConfig {
    fn default() -> Self {
        Self {
            mode: AutoScrollMode::default(),
            fixed_indicator_position: 200,
            page_trigger_offset: 100,
            page_return_position: 200,
        }
    }
}

impl std::fmt::Display for AutoScrollMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutoScrollMode::FixedIndicatorLeft => write!(f, "固定指示线 (卷帘滚动)"),
            AutoScrollMode::ScrollingIndicator => write!(f, "滚动指示线 (自动翻页)"),
            AutoScrollMode::Off => write!(f, "关闭"),
        }
    }
}
