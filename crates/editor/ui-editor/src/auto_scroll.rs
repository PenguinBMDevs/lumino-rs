use crate::Editor;
use lumino_core::storage::config::{AutoScrollConfig, AutoScrollMode};
use lumino_core::view_state::DEFAULT_KEYBOARD_WIDTH;

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
    /// 纵向卷帘（`editor_state.is_vertical_roll`）把时间轴转置到 Y 方向：
    /// 用 `zoom_y` 作时间轴缩放、`keyboard_height`(=横向 `keyboard_width`) 作 pitch 轴留白，
    /// 继续驱动 `scroll_x`（纵向视图的时间轴偏移），保证瀑布流「向下落」方向正确。
    ///
    /// 返回是否需要刷新网格缓存
    pub fn update_auto_scroll(&mut self, playback_tick: f32) -> bool {
        let asc = &self.editor_state.auto_scroll;
        if asc.mode == AutoScrollMode::Off {
            return false;
        }

        let is_vertical = self.editor_state.is_vertical_roll;
        let v = &self.editor_state.view;
        // 纵向卷帘等价于"把横向卷帘整体转 90°"：主滚动轴仍是 X（时间轴、键盘 pitch 轴、
        // auto_scroll 全部共用 `zoom_x`/`scroll_x`）。故纵向模式下时间轴缩放仍用 `zoom_x`，
        // 仅把 pitch 轴留白换成键盘高、画布尺寸换成画布高度。之前误用 `zoom_y` 导致与网格
        // 时间轴 `zoom_x` 错配，播放时 `scroll_x` 被按 `zoom_y` 驱动、网格按 `zoom_x` 解释，
        // 整片网格被推出可视区而"消失"。
        let time_zoom = v.zoom_x;
        let pitch_inset = if is_vertical {
            DEFAULT_KEYBOARD_WIDTH
        } else {
            v.keyboard_width
        };
        let canvas_size = if is_vertical {
            self.editor_state.canvas.size_y
        } else {
            self.editor_state.canvas.size_x
        };
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
                let target_scroll_x = playback_tick * time_zoom - indicator_pos;

                // 如果目标滚动已到达或超过末尾，自动松开固定
                // 此时滚动停在末尾，指示线自然跟随播放位置移动
                if target_scroll_x >= max_scroll {
                    self.set_scroll_x(max_scroll, pitch_inset, canvas_size, time_zoom);
                } else {
                    self.set_scroll_x(target_scroll_x, pitch_inset, canvas_size, time_zoom);
                }
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
                let indicator_screen_x = playback_tick * time_zoom - v.scroll_x + pitch_inset;
                let trigger_screen_x = viewport_width + pitch_inset - trigger_offset;

                if indicator_screen_x >= trigger_screen_x {
                    let target_scroll_x = playback_tick * time_zoom - return_pos;
                    self.set_scroll_x(target_scroll_x, pitch_inset, canvas_size, time_zoom);
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

    /// 获取演奏指示线在 Canvas 坐标系中的 Y 坐标（用于渲染，纵向卷帘）
    ///
    /// 与 `get_playback_indicator_screen_x` 对称：时间轴转置到 Y 方向，
    /// 用 `zoom_x` 作时间轴缩放（与网格时间轴一致）、`keyboard_height`(=横向 `keyboard_width`) 作顶部留白。
    pub fn get_playback_indicator_screen_y(&self) -> Option<f32> {
        let v = &self.editor_state.view;
        let asc = &self.editor_state.auto_scroll;
        let keyboard_height = DEFAULT_KEYBOARD_WIDTH;
        match asc.mode {
            AutoScrollMode::FixedIndicatorLeft => {
                let indicator_pos = asc.fixed_indicator_position as f32;

                let total_height = v.total_ticks as f32 * v.zoom_x;
                let viewport_height = (self.editor_state.canvas.size_y - keyboard_height).max(0.0);
                let max_scroll = (total_height - viewport_height).max(0.0);

                if max_scroll > 0.0 && v.scroll_x >= max_scroll - 1.0 {
                    let indicator_y =
                        self.playback_position * v.zoom_x - v.scroll_x + keyboard_height;
                    Some(indicator_y)
                } else {
                    Some(keyboard_height + indicator_pos)
                }
            }
            AutoScrollMode::ScrollingIndicator | AutoScrollMode::Off => {
                let indicator_y = self.playback_position * v.zoom_x - v.scroll_x + keyboard_height;
                Some(indicator_y)
            }
        }
    }
}
