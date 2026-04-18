use crate::editor::Editor;
use lumino_core::storage::config::{AutoScrollConfig, AutoScrollMode};

impl Editor {
    /// 设置自动滚动配置
    pub fn set_auto_scroll_config(&mut self, config: AutoScrollConfig) {
        self.auto_scroll_config = config;
    }

    /// 获取自动滚动配置
    pub fn auto_scroll_config(&self) -> &AutoScrollConfig {
        &self.auto_scroll_config
    }

    /// 循环切换自动滚动模式
    pub fn cycle_auto_scroll_mode(&mut self) {
        self.auto_scroll_config.mode = match self.auto_scroll_config.mode {
            AutoScrollMode::FixedIndicatorLeft => AutoScrollMode::ScrollingIndicator,
            AutoScrollMode::ScrollingIndicator => AutoScrollMode::Off,
            AutoScrollMode::Off => AutoScrollMode::FixedIndicatorLeft,
        };
        tracing::debug!(
            "Editor: 自动滚动模式切换为 {:?}",
            self.auto_scroll_config.mode
        );
    }

    /// 获取当前自动滚动模式
    pub fn auto_scroll_mode(&self) -> AutoScrollMode {
        self.auto_scroll_config.mode
    }

    /// 更新自动滚动（在每帧渲染前调用，根据播放位置调整滚动）
    /// 返回是否需要刷新网格缓存
    pub fn update_auto_scroll(&mut self, playback_tick: f32) -> bool {
        if self.auto_scroll_config.mode == AutoScrollMode::Off {
            return false;
        }

        let viewport_width = (self.canvas_size.x - self.state.keyboard_width).max(0.0);
        if viewport_width <= 0.0 {
            return false;
        }

        match self.auto_scroll_config.mode {
            AutoScrollMode::FixedIndicatorLeft => {
                let indicator_pos = self.auto_scroll_config.fixed_indicator_position as f32;
                let target_scroll_x = playback_tick * self.state.zoom_x - indicator_pos;
                self.set_scroll_x(target_scroll_x);
                true
            }
            AutoScrollMode::ScrollingIndicator => {
                let trigger_offset = self.auto_scroll_config.page_trigger_offset as f32;
                let return_pos = self.auto_scroll_config.page_return_position as f32;
                let indicator_screen_x = playback_tick * self.state.zoom_x - self.state.scroll_x
                    + self.state.keyboard_width;
                let trigger_screen_x = viewport_width + self.state.keyboard_width - trigger_offset;

                if indicator_screen_x >= trigger_screen_x {
                    let target_scroll_x = playback_tick * self.state.zoom_x - return_pos;
                    self.set_scroll_x(target_scroll_x);
                    return true;
                }
                false
            }
            AutoScrollMode::Off => false,
        }
    }

    /// 获取演奏指示线在 Canvas 坐标系中的 X 坐标（用于渲染）
    /// 返回 None 表示不需要显示指示线
    pub fn get_playback_indicator_screen_x(&self) -> Option<f32> {
        if self.auto_scroll_config.mode == AutoScrollMode::Off {
            return None;
        }

        match self.auto_scroll_config.mode {
            AutoScrollMode::FixedIndicatorLeft => {
                let indicator_pos = self.auto_scroll_config.fixed_indicator_position as f32;
                Some(self.state.keyboard_width + indicator_pos)
            }
            AutoScrollMode::ScrollingIndicator => {
                let indicator_x = self.playback_position * self.state.zoom_x - self.state.scroll_x
                    + self.state.keyboard_width;
                Some(indicator_x)
            }
            AutoScrollMode::Off => None,
        }
    }
}
