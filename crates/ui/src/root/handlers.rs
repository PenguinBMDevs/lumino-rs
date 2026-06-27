//! Root 消息处理器
//!
//! 采用分治法策略，将消息处理逻辑拆分为专门的处理器：
//! - CollaborationHandler: 协作功能
//! - DialogHandler: 对话框管理
//! - ToolbarHandler: 工具栏事件（播放控制统一通过此路径）
//!
//! 通过 MessageRouter 按顺序分发消息。

use crate::message::{EditorAction, Message};
use crate::root::Root;
use crate::{sidebar, window};

// 重新导出子模块
pub mod collaboration;
pub mod dialog;
pub mod loop_range;
pub mod toolbar;
pub mod velocity;

// 重新导出处理器类型
pub use collaboration::CollaborationHandler;
pub use dialog::DialogHandler;
pub use loop_range::LoopRangeHandler;
pub use toolbar::ToolbarHandler;
pub use velocity::VelocityHandler;

/// 消息处理器 trait
///
/// 实现此 trait 的类型可以处理特定类型的消息。
/// 如果返回 None，表示消息已处理完毕；
/// 如果返回 Some(msg)，表示消息需要传递给下一个处理器。
pub trait MessageHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message>;
}

/// 消息路由器
///
/// 按顺序将消息传递给各个处理器，直到有处理器处理完毕。
pub struct MessageRouter {
    handlers: Vec<Box<dyn MessageHandler>>,
}

impl MessageRouter {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    pub fn register(&mut self, handler: Box<dyn MessageHandler>) {
        self.handlers.push(handler);
    }

    pub fn route(&mut self, root: &mut Root, msg: Message) {
        let mut current_msg = Some(msg);

        for handler in &mut self.handlers {
            if let Some(msg) = current_msg {
                current_msg = handler.handle(root, msg);
            } else {
                break;
            }
        }

        // 如果还有未处理的消息，记录警告
        if let Some(unhandled) = current_msg {
            tracing::warn!("未处理的消息: {:?}", std::mem::discriminant(&unhandled));
        }
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl Root {
    /// 创建配置好的消息路由器
    pub fn create_message_router() -> MessageRouter {
        let mut router = MessageRouter::new();
        router.register(Box::new(CollaborationHandler::new()));
        router.register(Box::new(DialogHandler::new()));
        router.register(Box::new(VelocityHandler::new()));
        router.register(Box::new(LoopRangeHandler::new()));
        router.register(Box::new(ToolbarHandler::new()));
        router
    }

    /// 主更新入口 - 简化为路由分发
    pub fn update(&mut self, msg: Message) {
        // 每帧轮询 MIDI 输入缓冲区
        self.poll_midi_input();

        // 先尝试直接处理消息
        if self.try_handle_direct(&msg) {
            return;
        }

        // 将 router 从 self 中取出，避免 &mut self 与 &mut self.message_router 冲突
        // 使用临时空 router 占位，使用完毕后归还
        let mut router = std::mem::take(&mut self.message_router);
        router.route(self, msg);
        self.message_router = router;
    }

    /// 直接处理不需要路由的消息
    /// 返回 true 表示消息已被处理
    fn try_handle_direct(&mut self, msg: &Message) -> bool {
        match msg {
            Message::Core(event) => {
                self.handle_core_event(event.clone());
                true
            }
            Message::Window(event) => {
                self.handle_window_event(event.clone());
                true
            }
            Message::Sidebar(event) => {
                // 返回 handle_sidebar_event 的结果，让调用者知道是否需要重新渲染
                self.handle_sidebar_event(event.clone())
            }
            Message::EditorAction(action) => {
                self.handle_editor_action(action.clone());
                true
            }
            _ => self.try_handle_simple_state(msg),
        }
    }

    // ─── 子处理函数 ──────────────────────────────────────────────────────────

    /// 处理走带视图水平滚动
    fn handle_arrangement_scroll_x(&mut self, x: f32) -> bool {
        let vp = &mut self.arrangement_view.viewport;
        let canvas_w = vp.canvas_size.x.max(1.0);
        let max_tick = self
            .editor
            .editor_state
            .data
            .track_notes
            .values()
            .flat_map(|notes| notes.iter().map(|n| n.tick + n.length))
            .fold(0.0_f32, f32::max)
            .max(crate::constants::editor::DEFAULT_MIN_TICKS);
        let total_w = max_tick * vp.zoom_x;
        let max_scroll = (total_w - canvas_w).max(0.0);
        vp.scroll_x = x.max(0.0).min(max_scroll);
        true
    }

    /// 处理走带视图垂直滚动
    fn handle_arrangement_scroll_y(&mut self, y: f32) -> bool {
        let vp = &mut self.arrangement_view.viewport;
        let track_count = self.sidebar.tracks.len().max(1) as f32;
        let total_h = track_count * vp.track_height * vp.zoom_y;
        let canvas_h = vp.canvas_size.y.max(1.0);
        let max_scroll = (total_h - canvas_h).max(0.0);
        vp.scroll_y = y.max(0.0).min(max_scroll);
        true
    }

    /// 处理走带视图水平缩放（固定点缩放）
    fn handle_arrangement_zoom_x(&mut self, zoom: f32, fixed_ratio: f32) -> bool {
        let vp = &mut self.arrangement_view.viewport;
        let old_zoom = vp.zoom_x;
        let new_zoom = zoom.clamp(
            crate::constants::editor::zoom::MIN_ARRANGEMENT_ZOOM_X,
            crate::constants::editor::zoom::MAX_ARRANGEMENT_ZOOM_X,
        );
        let canvas_w = vp.canvas_size.x.max(1.0);
        let focus_px = vp.scroll_x + canvas_w * fixed_ratio;
        let focus_tick = focus_px / old_zoom;
        vp.zoom_x = new_zoom;
        vp.scroll_x = (focus_tick * new_zoom - canvas_w * fixed_ratio).max(0.0);
        true
    }

    /// 处理走带视图垂直缩放（固定点缩放）
    fn handle_arrangement_zoom_y(&mut self, zoom: f32, fixed_ratio: f32) -> bool {
        let vp = &mut self.arrangement_view.viewport;
        let old_zoom = vp.zoom_y;
        let canvas_h = vp.canvas_size.y.max(1.0);
        let track_count = self.sidebar.tracks.len().max(1) as f32;
        let min_zoom = crate::constants::editor::zoom::MIN_ARRANGEMENT_ZOOM_Y;
        let max_zoom = (canvas_h / (track_count * vp.track_height)).max(min_zoom);
        let new_zoom = zoom.clamp(min_zoom, max_zoom);
        let focus_px = vp.scroll_y + canvas_h * fixed_ratio;
        let focus_ratio = focus_px / (old_zoom * vp.track_height);
        vp.zoom_y = new_zoom;
        let total_h = track_count * vp.track_height * new_zoom;
        let max_scroll = (total_h - canvas_h).max(0.0);
        vp.scroll_y = (focus_ratio * new_zoom * vp.track_height - canvas_h * fixed_ratio)
            .clamp(0.0, max_scroll);
        true
    }

    /// 处理设置面板事件
    fn handle_settings_event(&mut self, event: &Message) -> bool {
        if let Message::Settings(event) = event {
            self.settings.update(event.clone());
            match event {
                crate::settings::Event::EraserBehaviorChanged(behavior) => {
                    self.editor.set_eraser_behavior(*behavior);
                }
                crate::settings::Event::SelectionBoxModeChanged(mode) => {
                    self.editor.set_selection_box_mode(*mode);
                    tracing::debug!("Root: 框选框模式切换为 {:?}", mode);
                }
                crate::settings::Event::VelocityFilterThresholdChanged(value) => {
                    if let Ok(val) = value.parse::<u8>() {
                        self.visual.velocity_filter_threshold = val;
                        tracing::debug!("Root: 力度过滤阈值同步为 {}", val);
                    }
                }
                crate::settings::Event::AutoScrollFixedPositionChanged(value) => {
                    if let Ok(val) = value.parse::<u32>() {
                        let mut config = *self.editor.auto_scroll_config();
                        config.fixed_indicator_position = val;
                        self.editor.set_auto_scroll_config(config);
                        tracing::debug!("Root: 自动滚动固定位置同步为 {}", val);
                    }
                }
                crate::settings::Event::AutoScrollPageTriggerOffsetChanged(value) => {
                    if let Ok(val) = value.parse::<u32>() {
                        let mut config = *self.editor.auto_scroll_config();
                        config.page_trigger_offset = val;
                        self.editor.set_auto_scroll_config(config);
                        tracing::debug!("Root: 自动滚动翻页触发偏移同步为 {}", val);
                    }
                }
                crate::settings::Event::AutoScrollPageReturnPositionChanged(value) => {
                    if let Ok(val) = value.parse::<u32>() {
                        let mut config = *self.editor.auto_scroll_config();
                        config.page_return_position = val;
                        self.editor.set_auto_scroll_config(config);
                        tracing::debug!("Root: 自动滚动翻页返回位置同步为 {}", val);
                    }
                }
                crate::settings::Event::IconHiDPIChanged(enabled) => {
                    crate::resources::icon::set_hidpi_enabled(*enabled);
                    tracing::debug!("Root: HiDPI 图标渲染切换为 {}", enabled);
                }
                crate::settings::Event::Enable256keyChanged(enabled) => {
                    let new_count: u16 = if *enabled { 256 } else { 128 };
                    self.editor.set_visible_key_count(new_count);
                    self.editor.editor_state.view.key_count = new_count;
                    tracing::debug!(
                        "Root: 256键模式切换为 {}，琴键数调整为 {}",
                        enabled,
                        new_count
                    );
                }
                crate::settings::Event::LanguageChanged(lang) => {
                    tracing::debug!("Root: 界面语言切换为 {:?}", lang);
                }
                _ => {} // 其他设置变更由 settings.update() 同步
            }
            true
        } else {
            false
        }
    }

    /// 处理模式切换（编辑器 ↔ 瀑布流）
    fn handle_mode_toggle(&mut self) -> bool {
        let target_mode = match self.state.current_mode {
            crate::titlebar::mode_toggle::AppMode::Editor => {
                crate::titlebar::mode_toggle::AppMode::Waterfall
            }
            crate::titlebar::mode_toggle::AppMode::Waterfall => {
                crate::titlebar::mode_toggle::AppMode::Editor
            }
        };
        let target_progress = match target_mode {
            crate::titlebar::mode_toggle::AppMode::Editor => 0.0,
            crate::titlebar::mode_toggle::AppMode::Waterfall => 1.0,
        };
        self.state.current_mode = target_mode;
        self.state.toggle_animation.animate_to(target_progress);
        true
    }

    /// 处理动画 tick（切换动画 + 平滑滚动 + 弹簧物理）
    fn handle_animation_tick(&mut self) -> bool {
        let still_animating = self.state.toggle_animation.update();
        if !still_animating
            && self.state.toggle_animation.position >= 0.5
            && self.state.current_mode != crate::titlebar::mode_toggle::AppMode::Waterfall
        {
            self.state.current_mode = crate::titlebar::mode_toggle::AppMode::Waterfall;
        } else if !still_animating
            && self.state.toggle_animation.position < 0.5
            && self.state.current_mode != crate::titlebar::mode_toggle::AppMode::Editor
        {
            self.state.current_mode = crate::titlebar::mode_toggle::AppMode::Editor;
        }

        let scroll_animating = {
            let state = &mut self.editor.editor_state;
            let v = &mut state.view;
            let (new_x, new_y, still_active) = v.smooth_scroll.update(v.scroll_x, v.scroll_y);
            // 钳制到有效滚动范围，防止平滑滚动超出键盘/音轨边界
            let max_y =
                (state.max_scroll.1 - (state.canvas.size_y - v.ruler_height).max(0.0)).max(0.0);
            let max_x =
                (state.max_scroll.0 - (state.canvas.size_x - v.keyboard_width).max(0.0)).max(0.0);
            v.scroll_x = new_x.clamp(0.0, max_x);
            v.scroll_y = new_y.clamp(0.0, max_y);
            still_active
        };
        if scroll_animating {
            self.editor
                .invalidate_caches(crate::editor::CacheInvalidation::ALL);
        }

        self.editor.update_selection_box_animation(None);
        true
    }

    /// 处理简单的状态更新消息
    fn try_handle_simple_state(&mut self, msg: &Message) -> bool {
        match msg {
            Message::Progress(progress) => {
                self.progress = progress.clone();
                true
            }
            Message::ScrollbarScrolled(x) => {
                self.editor.set_scroll_x(*x);
                true
            }
            Message::ScrollbarScrolledY(y) => {
                self.editor.set_scroll_y(*y);
                true
            }
            Message::ArrangementScrollX(x) => self.handle_arrangement_scroll_x(*x),
            Message::ArrangementScrollY(y) => self.handle_arrangement_scroll_y(*y),
            Message::ArrangementZoomX { zoom, fixed_ratio } => {
                self.handle_arrangement_zoom_x(*zoom, *fixed_ratio)
            }
            Message::ArrangementZoomY { zoom, fixed_ratio } => {
                self.handle_arrangement_zoom_y(*zoom, *fixed_ratio)
            }
            Message::ZoomXChanged { zoom, fixed_ratio } => {
                self.editor.set_zoom_x(*zoom, *fixed_ratio);
                true
            }
            Message::ZoomYChanged { zoom, fixed_ratio } => {
                self.editor.set_zoom_y(*zoom, *fixed_ratio);
                true
            }
            Message::CanvasBoundsChanged { offset, size } => {
                self.editor.set_canvas_offset(*offset);
                self.editor
                    .set_canvas_size(iced_core::Point::new(size.width, size.height));

                // 缩放不应出现空白区域：viewport 变化后，若琴键没填满 viewport，
                // 自动钳正 zoom_y，始终让 content 高度 ≥ viewport 高度。
                // 确保面板开关或 window resize 后不留空区。
                let state = &mut self.editor.editor_state;
                let vh = (state.canvas.size_y - state.view.ruler_height).max(0.0);
                let th = state.view.visible_key_count as f32 * state.view.zoom_y;

                if th < vh {
                    // content 没填满 viewport → 调高 zoom
                    let fill_zoom = (vh / state.view.visible_key_count as f32).clamp(
                        crate::constants::editor::zoom::MIN_ZOOM_Y,
                        crate::constants::editor::zoom::MAX_ZOOM_Y,
                    );
                    if (fill_zoom - state.view.zoom_y).abs() > f32::EPSILON {
                        state.view.zoom_y = fill_zoom;
                        state.update_max_scroll(state.view.total_ticks);
                    }
                }

                // 重新钳制滚动位置
                let ms_y = (state.max_scroll.1 - vh).max(0.0);
                state.view.scroll_y = state.view.scroll_y.min(ms_y);
                let vw = (state.canvas.size_x - state.view.keyboard_width).max(0.0);
                let ms_x = (state.max_scroll.0 - vw).max(0.0);
                state.view.scroll_x = state.view.scroll_x.min(ms_x);

                self.editor
                    .invalidate_caches(crate::editor::CacheInvalidation::KEYBOARD);
                true
            }
            Message::MenuStateChanged(is_open) => {
                self.state.is_menu_open = *is_open;
                true
            }
            Message::CtrlKeyChanged(pressed) => {
                self.toolbar.ctrl_pressed = *pressed;
                true
            }
            Message::ShiftKeyChanged(pressed) => {
                self.toolbar.shift_pressed = *pressed;
                true
            }
            Message::Settings(_) => self.handle_settings_event(msg),
            Message::ToggleSettings | Message::Null => true,
            Message::ModeToggled => self.handle_mode_toggle(),
            Message::AnimationTick => self.handle_animation_tick(),
            Message::VelocityPanelResize(height) => {
                self.visual.velocity_panel_height = *height;
                true
            }
            Message::PerformancePanelToggled => {
                self.statusbar.toggle_perf_panel();
                true
            }
            Message::PerfUpdate(data) => {
                self.statusbar.set_perf_data(*data);
                true
            }
            Message::MidiInputEvent { data } => {
                let data = data.clone();
                if let Ok(mut buf) = self.midi.input_buffer.lock() {
                    buf.push_back(data);
                }
                true
            }
            _ => false,
        }
    }

    // ====================================================================
    // 核心事件处理
    // ====================================================================

    fn handle_core_event(&mut self, event: crate::event::Event) {
        self.set_menu_open(false);
        crate::event::emit(event);
    }

    fn handle_window_event(&mut self, event: window::Event) {
        let is_fps_update = matches!(&event, window::Event::FpsUpdate(_));
        let is_theme_change = matches!(&event, window::Event::Theme(_));

        if is_fps_update && let window::Event::FpsUpdate(fps) = &event {
            self.statusbar.set_fps(*fps);
        }

        // PerfUpdate 通过 Message::Window(Event::PerfUpdate) 路由到此路径，
        // 直接转发到状态栏（否则被 window.update 吞没，数据显示全零）
        if let window::Event::PerfUpdate(data) = &event {
            self.statusbar.set_perf_data(*data);
        }

        self.window.update(event);

        if is_theme_change {
            self.editor.grid_cache.clear();
            self.editor.keyboard_cache.clear();
            self.editor.ruler_cache.clear();
        }
    }

    fn handle_sidebar_event(&mut self, event: sidebar::Event) -> bool {
        // 自动化面板切换始终触发重绘
        if matches!(&event, sidebar::Event::AutomationPanelToggled) {
            self.sidebar.update(event);
            return true;
        }

        // 钢琴卷帘切换始终触发重绘
        if matches!(&event, sidebar::Event::PianoRollToggled) {
            self.sidebar.update(event);
            return true;
        }

        // 先检查是否是音轨切换
        let track_selected_idx = if let sidebar::Event::TrackSelected(idx) = &event {
            Some(*idx)
        } else {
            None
        };

        // 更新 sidebar，获取是否需要重新渲染
        let needs_redraw = self.sidebar.update(event);

        // 更新画布偏移
        let sidebar_width = self.sidebar.width() as f32;
        let current_offset_y = self.editor.editor_state.canvas.offset_y;
        self.editor
            .set_canvas_offset(iced_core::Point::new(sidebar_width, current_offset_y));

        // 如果是音轨切换，发送 Core 事件
        if let Some(track_idx) = track_selected_idx {
            tracing::debug!("Root: 发射音轨选择事件，音轨 {}", track_idx);
            crate::event::emit(crate::event::Event::Menu(crate::event::menu::Event::File(
                crate::event::menu::file::Event::TrackSelected(track_idx),
            )));
        }

        needs_redraw
    }

    /// 处理编辑器动作
    pub(crate) fn handle_editor_action(&mut self, action: EditorAction) {
        let old_tick = self.editor.playback_position;
        self.editor.handle_action(action);
        let new_tick = self.editor.playback_position;

        // 检查播放位置是否变化
        if (old_tick - new_tick).abs() > f32::EPSILON
            && let Some(manager) = &mut self.playback.manager
        {
            manager.seek(new_tick);
        }

        // 检查音符数据是否变化
        if self.editor.notes_changed() {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
    }
}

#[cfg(test)]
mod handlers_tests;
