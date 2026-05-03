use crate::editor::Editor;
use lumino_core::storage::config::{AutoScrollConfig, AutoScrollMode};

impl Editor {
    /// 设置自动滚动配置
    pub fn set_auto_scroll_config(&mut self, config: AutoScrollConfig) {
        self.editor_state.auto_scroll = config;
    }

    /// 获取自动滚动配置
    pub fn auto_scroll_config(&self) -> &AutoScrollConfig {
        &self.editor_state.auto_scroll
    }

    /// 循环切换自动滚动模式
    pub fn cycle_auto_scroll_mode(&mut self) {
        let mode = self.editor_state.auto_scroll.mode;
        self.editor_state.auto_scroll.mode = match mode {
            AutoScrollMode::FixedIndicatorLeft => AutoScrollMode::ScrollingIndicator,
            AutoScrollMode::ScrollingIndicator => AutoScrollMode::Off,
            AutoScrollMode::Off => AutoScrollMode::FixedIndicatorLeft,
        };
        tracing::debug!(
            "Editor: 自动滚动模式切换为 {:?}",
            self.editor_state.auto_scroll.mode
        );
    }

    /// 获取当前自动滚动模式
    pub fn auto_scroll_mode(&self) -> AutoScrollMode {
        self.editor_state.auto_scroll.mode
    }

    /// 更新自动滚动（在每帧渲染前调用，根据播放位置调整滚动）
    /// 返回是否需要刷新网格缓存
    pub fn update_auto_scroll(&mut self, playback_tick: f32) -> bool {
        let asc = &self.editor_state.auto_scroll;
        if asc.mode == AutoScrollMode::Off {
            return false;
        }

        let v = &self.editor_state.view;
        let canvas_size = self.editor_state.canvas.size;
        let viewport_width = (canvas_size.x - v.keyboard_width).max(0.0);
        if viewport_width <= 0.0 {
            return false;
        }

        match asc.mode {
            AutoScrollMode::FixedIndicatorLeft => {
                let indicator_pos = asc.fixed_indicator_position as f32;
                let target_scroll_x = playback_tick * v.zoom_x - indicator_pos;
                self.set_scroll_x(target_scroll_x);
                true
            }
            AutoScrollMode::ScrollingIndicator => {
                let trigger_offset = asc.page_trigger_offset as f32;
                let return_pos = asc.page_return_position as f32;
                let indicator_screen_x = playback_tick * v.zoom_x - v.scroll_x
                    + v.keyboard_width;
                let trigger_screen_x = viewport_width + v.keyboard_width - trigger_offset;

                if indicator_screen_x >= trigger_screen_x {
                    let target_scroll_x = playback_tick * v.zoom_x - return_pos;
                    self.set_scroll_x(target_scroll_x);
                    return true;
                }
                false
            }
            AutoScrollMode::Off => false,
        }
    }

    /// 获取演奏指示线在 Canvas 坐标系中的 X 坐标（用于渲染）
    pub fn get_playback_indicator_screen_x(&self) -> Option<f32> {
        let v = &self.editor_state.view;
        let asc = &self.editor_state.auto_scroll;
        match asc.mode {
            AutoScrollMode::FixedIndicatorLeft => {
                let indicator_pos = asc.fixed_indicator_position as f32;
                Some(v.keyboard_width + indicator_pos)
            }
            AutoScrollMode::ScrollingIndicator | AutoScrollMode::Off => {
                // 滚动指示线模式和关闭自动滚动时，都使用相同的计算方式
                // 指示线位置 = 播放位置对应的像素 - 滚动偏移 + 键盘宽度
                let indicator_x = self.playback_position * v.zoom_x - v.scroll_x
                    + v.keyboard_width;
                Some(indicator_x)
            }
        }
    }
}
