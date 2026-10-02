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

    /// 计算全屏瀑布流播放器（`AppMode::Waterfall`）的滚动偏移——**恒为固定下落模式**。
    ///
    /// 播放器不绘制演奏指示线：底部键盘线本身就是落点线，故该线的 tick 恒等于播放位置，
    /// 即 `scroll_x = playback_tick × zoom_x`（偏移 0）。这样键位点亮与音符落点严格同刻
    /// （播放器离屏键盘的落键判定 tick = `scroll_x / zoom_x`），音符等速连续下落到键盘。
    ///
    /// **与卷帘的自动滚动模式彻底解耦**——这是本函数存在的唯一理由：
    /// 卷帘「自动翻页」（`ScrollingIndicator`）只在播放头触边时整屏跳变、
    /// 「关闭」（`Off`）时 `scroll_x` 完全不动。播放器若直接复用卷帘 `scroll_x`，
    /// 音符下落就会退化成「停住 → 跳页」，甚至完全静止。
    ///
    /// **刻意不使用 `fixed_indicator_position`**：播放器没有可见的指示线，键盘线即落点线，
    /// 任何非零偏移都会让键位在音符落到键盘之前提前点亮（观感为「跑调」）。
    pub fn waterfall_player_scroll_x(&self, playback_tick: f32) -> f32 {
        (playback_tick * self.editor_state.view.zoom_x).max(0.0)
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

#[cfg(test)]
mod tests {
    use crate::Editor;
    use lumino_core::storage::config::{AutoScrollConfig, AutoScrollMode};

    /// 播放器瀑布流恒为固定下落：滚动值只由播放位置与 X 缩放决定，与卷帘自动滚动模式无关。
    ///
    /// 回归场景：卷帘处于「自动翻页」（`ScrollingIndicator`，只在触边时整屏跳变）或
    /// 「关闭」（`Off`，`scroll_x` 完全不动）时，播放器此前复用卷帘 `scroll_x`，
    /// 表现为音符「停住 → 跳页」甚至彻底静止。
    #[test]
    fn test_waterfall_player_scroll_x_is_fixed_regardless_of_roll_mode() {
        let mut editor = Editor::new();
        editor.editor_state.view.zoom_x = 0.5;

        for mode in [
            AutoScrollMode::FixedIndicatorLeft,
            AutoScrollMode::ScrollingIndicator,
            AutoScrollMode::Off,
        ] {
            editor.set_auto_scroll_config(AutoScrollConfig {
                mode,
                fixed_indicator_position: 200,
                page_trigger_offset: 100,
                page_return_position: 50,
            });
            let scroll = editor.waterfall_player_scroll_x(1000.0);
            assert!(
                (scroll - 500.0).abs() < f32::EPSILON,
                "{mode:?} 模式下播放器滚动应为 1000×0.5=500（固定下落），实际 = {scroll}"
            );
        }
    }

    /// 落点线 tick 恒等于播放位置 ⇒ 键位点亮与音符落到键盘严格同刻。
    #[test]
    fn test_waterfall_player_landing_line_equals_playback_tick() {
        let mut editor = Editor::new();
        editor.editor_state.view.zoom_x = 0.25;
        editor.editor_state.auto_scroll.mode = AutoScrollMode::ScrollingIndicator;

        let scroll = editor.waterfall_player_scroll_x(2400.0);
        let landing_tick = scroll / editor.editor_state.view.zoom_x;
        assert!(
            (landing_tick - 2400.0).abs() < f32::EPSILON,
            "落点线 tick 应等于播放位置 2400，实际 = {landing_tick}"
        );
    }

    /// 原 BUG 机制复现：卷帘「自动翻页」期间卷帘 `scroll_x` 长时间冻结（只在触边时整屏跳变），
    /// 而播放器滚动必须逐帧等速推进。
    ///
    /// 旧实现让播放器直接复用卷帘 `scroll_x`，于是播放器的音符在多数帧里「停住」、
    /// 少数帧里「跳页」——即用户报的「下落逻辑跟着卷帘翻页模式走」。本例把两侧的逐帧
    /// 行为同时量化：卷帘有冻结帧（证明翻页语义确实存在），播放器零冻结、步长恒定。
    #[test]
    fn test_waterfall_player_scroll_advances_while_roll_page_stays_frozen() {
        let mut editor = Editor::new();
        editor.editor_state.view.zoom_x = 2.0;
        editor.editor_state.view.keyboard_width = 60.0;
        editor.editor_state.canvas.size_x = 1280.0;
        editor.editor_state.view.total_ticks = 100_000;
        editor.set_auto_scroll_config(AutoScrollConfig {
            mode: AutoScrollMode::ScrollingIndicator,
            page_trigger_offset: 100,
            page_return_position: 200,
            ..Default::default()
        });

        const STEPS: u32 = 20;
        const TICK_STEP: f32 = 100.0;
        const EXPECTED_PLAYER_DELTA: f32 = TICK_STEP * 2.0; // = tick 步长 × zoom_x

        let mut prev_player_scroll = editor.waterfall_player_scroll_x(1000.0);
        let mut prev_roll_scroll = editor.editor_state.view.scroll_x;
        let mut roll_frozen_frames = 0u32;
        let mut player_frozen_frames = 0u32;
        let mut player_delta_violations = 0u32;

        for step in 1..=STEPS {
            let tick = 1000.0 + step as f32 * TICK_STEP;
            editor.playback_position = tick;
            // 卷帘侧自动滚动照常运行（翻页模式：只在触边时整块跳变）
            editor.update_auto_scroll(tick, true);

            let roll_scroll = editor.editor_state.view.scroll_x;
            if (roll_scroll - prev_roll_scroll).abs() < f32::EPSILON {
                roll_frozen_frames += 1;
            }
            prev_roll_scroll = roll_scroll;

            let player_scroll = editor.waterfall_player_scroll_x(tick);
            let delta = player_scroll - prev_player_scroll;
            if delta <= 0.0 {
                player_frozen_frames += 1;
            } else if (delta - EXPECTED_PLAYER_DELTA).abs() > f32::EPSILON {
                player_delta_violations += 1;
            }
            prev_player_scroll = player_scroll;
        }

        assert!(
            roll_frozen_frames > 0,
            "翻页模式下卷帘 scroll_x 应有冻结帧（本用例前提），实际冻结 0 帧"
        );
        assert_eq!(
            player_frozen_frames, 0,
            "播放器固定下落不得有冻结帧（旧实现复用卷帘 scroll_x 时正是如此）"
        );
        assert_eq!(
            player_delta_violations, 0,
            "播放器应等速下落：每步位移恒为 {EXPECTED_PLAYER_DELTA}px"
        );
    }

    /// 越界负 tick 钳到 0：绝不产出负滚动（否则落点线会掉到键盘线以下、音符错位）。
    #[test]
    fn test_waterfall_player_scroll_x_clamps_negative() {
        let editor = Editor::new();
        assert_eq!(editor.waterfall_player_scroll_x(-10.0), 0.0);
    }
}
