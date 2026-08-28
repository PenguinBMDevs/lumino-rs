use crate::Editor;
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
            "Editor: 自动滚动模式切换 {:?}",
            self.editor_state.auto_scroll.mode
        );
    }

    /// 获取当前自动滚动模式
    pub fn auto_scroll_mode(&self) -> AutoScrollMode {
        self.editor_state.auto_scroll.mode
    }

    /// 更新自动滚动（在每帧渲染前调用，根据播放位置调整滚动）
    ///
    /// `is_playing` 指示当前是否处于播放状态。**自动翻页（模式2 `ScrollingIndicator`）
    /// 仅在播放状态下触发**——非播放状态（暂停/停止/拖拽预览/seek）下若仍按播放头位置
    /// 翻页，会打断用户对视图滚动的手动控制，导致滚动异常。
    ///
    /// 返回是否需要刷新网格缓存
    pub fn update_auto_scroll(&mut self, playback_tick: f32, is_playing: bool) -> bool {
        let asc = &self.editor_state.auto_scroll;
        if asc.mode == AutoScrollMode::Off {
            return false;
        }

        let v = &self.editor_state.view;
        let time_zoom = v.zoom_x;
        let pitch_inset = v.keyboard_width;
        let canvas_size = self.editor_state.canvas.size_x;
        let viewport_width = (canvas_size - pitch_inset).max(0.0);
        if viewport_width <= 0.0 {
            return false;
        }

        // 计算最大滚动
        let total_width = v.total_ticks as f32 * time_zoom;
        let max_scroll = (total_width - viewport_width).max(0.0);

        match asc.mode {
            AutoScrollMode::FixedIndicatorLeft => {
                let indicator_pos = asc.fixed_indicator_position as f32;
                let mut target_scroll_x = playback_tick * time_zoom - indicator_pos;

                // 如果目标滚动已到达或超过末尾，自动松开固定
                // 此时滚动停在末尾，指示线自然跟随播放位置移动
                if target_scroll_x >= max_scroll {
                    target_scroll_x = max_scroll;
                }
                // 纵向卷帘时间轴在 Y、内容应「瀑布流下落」(scroll 增→坐标增→下移)，
                // 与横向同取 +target；竖向滚动条滑块与 scroll 同向（0=起点在底，随播放上移）。
                let scroll = target_scroll_x;
                self.set_scroll_x(scroll, pitch_inset, canvas_size, time_zoom);
                // 自动滚动直接设置，同步平滑滚动目标
                self.editor_state.view.smooth_scroll.sync(
                    self.editor_state.view.scroll_x,
                    self.editor_state.view.scroll_y,
                );
                true
            }
            AutoScrollMode::ScrollingIndicator => {
                let trigger_offset = asc.page_trigger_offset as f32;
                let return_pos = asc.page_return_position as f32;
                // 横向/纵向的屏幕坐标公式一致：ruler + tick*zoom - scroll（纵向 scroll 已为正，下落方向）
                let indicator_screen_x = playback_tick * time_zoom - v.scroll_x + pitch_inset;
                let trigger_screen_x = viewport_width + pitch_inset - trigger_offset;

                // 仅在播放状态下触发自动翻页：非播放状态（暂停/停止/拖拽预览/seek）下
                // 不主动翻页，避免打断用户对视图滚动的手动控制、造成滚动异常。
                if is_playing && indicator_screen_x >= trigger_screen_x {
                    let mut target_scroll_x = playback_tick * time_zoom - return_pos;
                    if target_scroll_x >= max_scroll {
                        target_scroll_x = max_scroll;
                    }
                    let scroll = target_scroll_x;
                    self.set_scroll_x(scroll, pitch_inset, canvas_size, time_zoom);
                    // 自动滚动直接设置，同步平滑滚动目标
                    self.editor_state.view.smooth_scroll.sync(
                        self.editor_state.view.scroll_x,
                        self.editor_state.view.scroll_y,
                    );
                    return true;
                }
                false
            }
            AutoScrollMode::Off => false,
        }
    }

    /// 获取演奏指示线在 Canvas 坐标系中的 X 坐标（用于渲染，横向卷帘）
    pub fn get_playback_indicator_screen_x(&self) -> Option<f32> {
        let v = &self.editor_state.view;
        let asc = &self.editor_state.auto_scroll;
        match asc.mode {
            AutoScrollMode::FixedIndicatorLeft => {
                let indicator_pos = asc.fixed_indicator_position as f32;

                // 检查滚动是否已到达末尾（无法再保持固定位置）
                let total_width = v.total_ticks as f32 * v.zoom_x;
                let viewport_width = (self.editor_state.canvas.size_x - v.keyboard_width).max(0.0);
                let max_scroll = (total_width - viewport_width).max(0.0);

                if max_scroll > 0.0 && v.scroll_x >= max_scroll - 1.0 {
                    // 已到达结尾：指示线跟随播放位置自然移动
                    let indicator_x =
                        self.playback_position * v.zoom_x - v.scroll_x + v.keyboard_width;
                    Some(indicator_x)
                } else {
                    Some(v.keyboard_width + indicator_pos)
                }
            }
            AutoScrollMode::ScrollingIndicator | AutoScrollMode::Off => {
                // 滚动指示线模式和关闭自动滚动时，都使用相同的计算方式
                // 指示线位置 = 播放位置对应的像素 - 滚动偏移 + 键盘宽度
                let indicator_x = self.playback_position * v.zoom_x - v.scroll_x + v.keyboard_width;
                Some(indicator_x)
            }
        }
    }
}
