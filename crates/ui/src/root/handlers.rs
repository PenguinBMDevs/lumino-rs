//! Root 消息处理器
//!
//! 采用分治法策略，将消息处理逻辑拆分为专门的处理器：
//! - CollaborationHandler: 协作功能
//! - DialogHandler: 对话框管理
//! - ToolbarHandler: 工具栏事件（播放控制统一通过此路径）
//!
//! 通过 MessageRouter 按顺序分发消息。

use crate::message::{EditorAction, LoopRangeAction, Message};
use crate::root::Root;
use crate::{sidebar, window};

// 重新导出子模块
pub mod collaboration;
pub mod dialog;
pub mod toolbar;

// 重新导出处理器类型
pub use collaboration::CollaborationHandler;
pub use dialog::DialogHandler;
pub use toolbar::ToolbarHandler;

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
        router.register(Box::new(ToolbarHandler::new()));
        router
    }

    /// 主更新入口 - 简化为路由分发
    pub fn update(&mut self, msg: Message) {
        // 每帧轮询 MIDI 输入缓冲区
        self.poll_midi_input();

        // 使用消息路由器处理
        let mut router = Self::create_message_router();
        let remaining = self.route_message(msg, &mut router);

        // 处理未被专门处理器处理的消息
        if let Some(msg) = remaining {
            self.handle_remaining_messages(msg);
        }
    }

    /// 路由消息到专门的处理器
    fn route_message(&mut self, msg: Message, router: &mut MessageRouter) -> Option<Message> {
        // 尝试直接处理消息
        if self.try_handle_direct(&msg) {
            return None;
        }

        // 其他消息通过路由器处理
        router.route(self, msg);
        None
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
            Message::Velocity(action) => {
                self.handle_velocity_action(action.clone());
                true
            }
            _ => self.try_handle_simple_state(msg),
        }
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
            Message::ArrangementScrollX(x) => {
                let vp = &mut self.arrangement_view.viewport;
                let canvas_w = vp.canvas_size.x.max(1.0);
                // 从实际音符数据计算总宽度
                let max_tick = self
                    .editor
                    .editor_state
                    .data
                    .track_notes
                    .values()
                    .flat_map(|notes| notes.iter().map(|n| n.tick + n.length))
                    .fold(0.0_f32, f32::max)
                    .max(960.0 * 4.0);
                let total_w = max_tick * vp.zoom_x;
                let max_scroll = (total_w - canvas_w).max(0.0);
                let clamped = x.max(0.0).min(max_scroll);
                vp.scroll_x = clamped;
                true
            }
            Message::ArrangementScrollY(y) => {
                let vp = &mut self.arrangement_view.viewport;
                let track_count = self.sidebar.tracks.len().max(1) as f32;
                let total_h = track_count * vp.track_height * vp.zoom_y;
                let canvas_h = vp.canvas_size.y.max(1.0);
                let max_scroll = (total_h - canvas_h).max(0.0);
                let clamped = y.max(0.0).min(max_scroll);
                vp.scroll_y = clamped;
                true
            }
            Message::ArrangementZoomX { zoom, fixed_ratio } => {
                let vp = &mut self.arrangement_view.viewport;
                let old_zoom = vp.zoom_x;
                let new_zoom = zoom.clamp(0.01, 10.0);
                let canvas_w = vp.canvas_size.x.max(1.0);
                // 保持固定点 tick 不变
                let focus_px = vp.scroll_x + canvas_w * fixed_ratio;
                let focus_tick = focus_px / old_zoom;
                vp.zoom_x = new_zoom;
                vp.scroll_x = (focus_tick * new_zoom - canvas_w * fixed_ratio).max(0.0);
                true
            }
            Message::ArrangementZoomY { zoom, fixed_ratio } => {
                let vp = &mut self.arrangement_view.viewport;
                let old_zoom = vp.zoom_y;
                let canvas_h = vp.canvas_size.y.max(1.0);
                // 最大缩放：最后一个音轨完全显示
                let track_count = self.sidebar.tracks.len().max(1) as f32;
                let max_zoom = (canvas_h / (track_count * vp.track_height)).max(0.2);
                let new_zoom = zoom.clamp(0.2, max_zoom);
                // 保持固定点位置不变
                let focus_px = vp.scroll_y + canvas_h * fixed_ratio;
                let focus_ratio = focus_px / (old_zoom * vp.track_height);
                vp.zoom_y = new_zoom;
                let total_h = track_count * vp.track_height * new_zoom;
                let max_scroll = (total_h - canvas_h).max(0.0);
                vp.scroll_y =
                    (focus_ratio * new_zoom * vp.track_height - canvas_h * fixed_ratio)
                        .clamp(0.0, max_scroll);
                true
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
                // 画布大小变化影响视口范围，洋葱皮缓存需失效
                self.invalidate_onion_skin_cache();
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
            Message::Settings(event) => {
                self.settings.update(event.clone());
                match &event {
                    crate::settings::Event::EraserBehaviorChanged(behavior) => {
                        self.editor.set_eraser_behavior(*behavior);
                    }
                    crate::settings::Event::SelectionBoxModeChanged(mode) => {
                        self.editor.set_selection_box_mode(*mode);
                        tracing::debug!("Root: 框选框模式切换为 {:?}", mode);
                    }
                    crate::settings::Event::VelocityFilterThresholdChanged(value) => {
                        if let Ok(val) = value.parse::<u8>() {
                            self.velocity_filter_threshold = val;
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
                        // 同步更新 key_count 字段保持一致性
                        self.editor.editor_state.view.key_count = new_count;
                        tracing::debug!(
                            "Root: 256键模式切换为 {}，琴键数调整为 {}",
                            enabled,
                            new_count
                        );
                    }
                    _ => {}
                }
                true
            }
            Message::ToggleSettings | Message::Null => true,
            Message::ModeToggled => {
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
            Message::AnimationTick => {
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

                // 更新钢琴卷帘平滑滚动动画
                let scroll_animating = self.editor.editor_state.update_smooth_scroll();
                if scroll_animating {
                    // 滚动变化需要重绘所有缓存
                    self.editor
                        .invalidate_caches(crate::editor::CacheInvalidation::ALL);
                }

                // 更新框选框弹簧物理动画（无鼠标移动时持续推进收敛）
                self.editor.update_selection_box_animation(None);

                true
            }
            Message::VelocityPanelResize(height) => {
                self.velocity_panel_height = *height;
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
            Message::LoopRange(action) => {
                self.handle_loop_range_action(action.clone());
                true
            }
            Message::MidiInputEvent { data } => {
                let data = data.clone();
                // 将 MIDI 数据放入缓冲区等待 poll_midi_input 处理
                if let Ok(mut buf) = self.midi_input_buffer.lock() {
                    buf.push_back(data);
                }
                true
            }
            _ => false,
        }
    }

    /// 处理剩余的消息（备用）
    fn handle_remaining_messages(&mut self, msg: Message) {
        // 未处理的消息（通常意味着需要扩展处理器）
        let _ = std::mem::discriminant(&msg);
    }

    // ====================================================================
    // 核心事件处理
    // ====================================================================

    fn handle_core_event(&mut self, event: lumino_core::event::Event) {
        self.set_menu_open(false);
        lumino_core::event::emit(event);
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
            // 主题变化只影响颜色，走快速路径
            self.invalidate_onion_skin_colors();
        }
    }

    fn handle_sidebar_event(&mut self, event: sidebar::Event) -> bool {
        // 先检查是否是音轨切换
        let track_selected_idx = if let sidebar::Event::TrackSelected(idx) = &event {
            Some(*idx)
        } else {
            None
        };

        // 检查是否是洋葱皮开关
        let onion_skin_toggled = matches!(&event, sidebar::Event::TrackOnionSkinToggled(_));

        // 更新 sidebar，获取是否需要重新渲染
        let needs_redraw = self.sidebar.update(event);

        // 更新画布偏移
        let sidebar_width = self.sidebar.width() as f32;
        let current_offset = self.editor.editor_state.canvas.offset;
        self.editor
            .set_canvas_offset(iced_core::Point::new(sidebar_width, current_offset.y));

        // 洋葱皮开关变化，使缓存失效
        if onion_skin_toggled {
            self.invalidate_onion_skin_cache();
        }

        // 如果是音轨切换，发送 Core 事件
        if let Some(track_idx) = track_selected_idx {
            tracing::debug!("Root: 发射音轨选择事件，音轨 {}", track_idx);
            lumino_core::event::emit(lumino_core::event::Event::Menu(
                lumino_core::event::menu::Event::File(
                    lumino_core::event::menu::file::Event::TrackSelected(track_idx),
                ),
            ));
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
            && let Some(manager) = &mut self.playback_manager
        {
            manager.seek(new_tick);
        }

        // 检查音符数据是否变化
        if self.editor.notes_changed() {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
            // 音符变化影响洋葱皮缓存
            self.invalidate_onion_skin_cache();
        }
    }

    /// 处理力度编辑面板动作
    pub(crate) fn handle_velocity_action(&mut self, action: crate::message::VelocityAction) {
        use crate::message::VelocityAction;

        match action {
            VelocityAction::DragStart(note_index, velocity) => {
                self.editor.push_history();
                Self::apply_velocity(&mut self.editor, note_index, velocity);
            }
            VelocityAction::DragMove(note_index, new_velocity) => {
                Self::apply_velocity(&mut self.editor, note_index, new_velocity);
            }
            VelocityAction::DragEnd => {
                tracing::debug!("力度面板: 拖拽结束");
            }
            VelocityAction::CurveStart => {
                self.editor.push_history();
                tracing::debug!("力度面板: 曲线绘制开始");
            }
            VelocityAction::CurvePaint(updates) => {
                for (note_index, velocity) in updates {
                    Self::apply_velocity(&mut self.editor, note_index, velocity);
                }
            }
            VelocityAction::CurveEnd => {
                tracing::debug!("力度面板: 曲线绘制结束");
            }
        }

        // 同步播放引擎：力度修改必须实时反映到播放中
        if self.editor.notes_changed() {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
            self.invalidate_onion_skin_cache();
        }
    }

    /// 应用力度值到指定音符，仅在力度实际变化时标记音符变更
    fn apply_velocity(editor: &mut crate::editor::Editor, note_index: usize, velocity: u8) {
        let data = &mut editor.editor_state.data;
        if note_index < data.notes.len()
            && let Some(note) = data.notes.get_mut(note_index)
        {
            let clamped = velocity.clamp(0, 127);
            if note.velocity != clamped {
                note.velocity = clamped;
                editor.mark_notes_changed();
                tracing::debug!("力度面板: 音符[{}] 力度更新为 {}", note_index, clamped);
            }
        }
    }

    fn handle_loop_range_action(&mut self, action: LoopRangeAction) {
        match action {
            LoopRangeAction::Toggle => {
                let enabled = if let Some(loop_range) = &mut self.editor.loop_range {
                    loop_range.toggle();
                    self.editor.ruler_cache.clear();
                    loop_range.enabled()
                } else {
                    false
                };
                self.sync_loop_to_playback_state(enabled);
                tracing::info!("Root: 循环区域切换为 {}", enabled);
            }
            LoopRangeAction::SetRange(start, end) => {
                let (enabled, start_tick, end_tick) =
                    if let Some(loop_range) = &mut self.editor.loop_range {
                        loop_range.set_range(start, end);
                        if !loop_range.enabled() {
                            loop_range.enable();
                        }
                        self.editor.ruler_cache.clear();
                        (
                            loop_range.enabled(),
                            loop_range.start_tick(),
                            loop_range.end_tick(),
                        )
                    } else {
                        return;
                    };
                self.sync_loop_to_playback_with_range(enabled, start_tick, end_tick);
                tracing::info!("Root: 循环范围设置为 [{:.2}, {:.2}]", start, end);
            }
            LoopRangeAction::Clear => {
                if let Some(loop_range) = &mut self.editor.loop_range {
                    loop_range.disable();
                    self.sync_loop_to_playback_state(false);
                    self.editor.ruler_cache.clear();
                    tracing::info!("Root: 循环区域已清除");
                }
            }
            LoopRangeAction::RulerPressed { x, y: _ } => {
                if let Some(loop_range) = &mut self.editor.loop_range {
                    let view = &self.editor.editor_state.view;
                    let hit = loop_range.handle_mouse_press(
                        x,
                        view.keyboard_width,
                        view.scroll_x,
                        view.zoom_x,
                        view.ruler_height,
                        view.snap_precision,
                    );
                    if hit != crate::editor::grid::LoopHitTest::None {
                        self.editor.ruler_cache.clear();
                        tracing::debug!("Root: 标尺循环区域点击检测: {:?}", hit);
                    }
                }
            }
            LoopRangeAction::RulerMoved { x, y: _ } => {
                let should_sync = if let Some(loop_range) = &mut self.editor.loop_range {
                    if loop_range.is_dragging() {
                        let view = &self.editor.editor_state.view;
                        loop_range.handle_mouse_move(
                            x,
                            view.keyboard_width,
                            view.scroll_x,
                            view.zoom_x,
                            view.snap_precision,
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if should_sync {
                    let (enabled, start, end) = self.get_loop_range_state();
                    self.sync_loop_to_playback_with_range(enabled, start, end);
                    self.editor.ruler_cache.clear();
                }
            }
            LoopRangeAction::RulerReleased => {
                if let Some(loop_range) = &mut self.editor.loop_range
                    && loop_range.is_dragging()
                {
                    let start = loop_range.start_tick();
                    let end = loop_range.end_tick();
                    loop_range.handle_mouse_release();
                    self.editor.ruler_cache.clear();
                    tracing::debug!("Root: 循环拖拽释放，范围 [{:.2}, {:.2}]", start, end);
                }
            }
            LoopRangeAction::RulerDoubleClicked { x: _, y: _ } => {
                let enabled = if let Some(loop_range) = &mut self.editor.loop_range {
                    loop_range.toggle();
                    self.editor.ruler_cache.clear();
                    loop_range.enabled()
                } else {
                    false
                };
                self.sync_loop_to_playback_state(enabled);
                tracing::info!("Root: 标尺双击切换循环为 {}", enabled);
            }
        }
    }

    fn get_loop_range_state(&self) -> (bool, f32, f32) {
        self.editor
            .loop_range
            .as_ref()
            .map_or((false, 0.0, 0.0), |lr| {
                (lr.enabled(), lr.start_tick(), lr.end_tick())
            })
    }

    fn sync_loop_to_playback_state(&mut self, enabled: bool) {
        if let Some(manager) = &mut self.playback_manager {
            manager.set_looping(enabled);
            if enabled {
                if let Some(lr) = &self.editor.loop_range {
                    manager.set_loop_range(lr.start_tick(), lr.end_tick());
                }
            } else {
                manager.clear_loop_range();
            }
        }
        self.toolbar.is_looping = enabled;
    }

    fn sync_loop_to_playback_with_range(&mut self, enabled: bool, start: f32, end: f32) {
        if let Some(manager) = &mut self.playback_manager {
            manager.set_looping(enabled);
            if enabled {
                manager.set_loop_range(start, end);
            } else {
                manager.clear_loop_range();
            }
        }
        self.toolbar.is_looping = enabled;
    }
}
