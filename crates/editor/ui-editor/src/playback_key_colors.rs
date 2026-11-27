//! 播放时的键盘着色状态（从 `Editor` 拆出的第一个状态 bundle，T-30 slice）。

/// 演奏键色缓存：256 个键 × RGBA。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlaybackKeyColors {
    /// 固定大小数组：key（0-255）× RGBA，全零表示不着色。
    pub(crate) colors: [u8; 1024],
    /// 关闭时跳过扫描与清空，节省 CPU。
    pub(crate) enabled: bool,
}

impl Default for PlaybackKeyColors {
    fn default() -> Self {
        Self {
            colors: [0u8; 1024],
            enabled: false,
        }
    }
}

impl PlaybackKeyColors {
    /// 设置开关；返回本次是否真正清空了颜色。
    pub(crate) fn set_enabled(&mut self, enabled: bool) -> bool {
        self.enabled = enabled;
        if enabled { false } else { self.clear() }
    }

    /// 清空颜色；返回是否发生了变化。
    pub(crate) fn clear(&mut self) -> bool {
        if self.colors == [0u8; 1024] {
            return false;
        }
        self.colors = [0u8; 1024];
        true
    }
}
