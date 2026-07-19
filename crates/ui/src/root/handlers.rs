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
use lumino_core::storage::config::TrackAddBehavior;

// 重新导出子模块
pub mod collaboration;
pub mod dialog;
pub mod loop_range;
pub mod settings;
pub mod toolbar;
pub mod velocity;

// 重新导出处理器类型
pub use collaboration::CollaborationHandler;
pub use dialog::DialogHandler;
pub use loop_range::LoopRangeHandler;
pub use settings::SettingsHandler;
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

/// 创建配置好的消息路由器
pub fn create_message_router() -> MessageRouter {
    let mut router = MessageRouter::new();
    router.register(Box::new(CollaborationHandler::new()));
    router.register(Box::new(DialogHandler::new()));
    router.register(Box::new(SettingsHandler::new()));
    router.register(Box::new(VelocityHandler::new()));
    router.register(Box::new(LoopRangeHandler::new()));
    router.register(Box::new(ToolbarHandler::new()));
    router
}

impl Root {
    /// 直接处理不需要路由的消息
    /// 返回 true 表示消息已被处理
    pub(crate) fn try_handle_direct(&mut self, msg: &Message) -> bool {
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
            Message::PianoRollContextMenu(action) => {
                self.handle_piano_roll_context_menu(action.clone());
                true
            }
            _ => self.try_handle_simple_state(msg),
        }
    }

    /// 处理单条消息（供测试与内部调用复用）
    ///
    /// 复刻应用运行时 `Host::route_message` 的分发组合：先尝试 `Root` 直接处理，
    /// 未处理的消息再交给 `MessageRouter` 路由到各专用处理器（Toolbar/Dialog/...）。
    /// 与运行时使用同一套分发逻辑，保证测试与产品行为一致。
    pub fn update(&mut self, message: Message) {
        // PPQ 编辑时：任何非 PPQ 编辑消息触发时自动确认（实现"点击任意 UI 位置保存"）
        if self.toolbar.ppq_editing {
            let is_ppq_edit_msg = matches!(
                &message,
                Message::Toolbar(crate::toolbar::Event::PpqEditChanged(_))
                    | Message::Toolbar(crate::toolbar::Event::PpqEditToggled(_))
                    | Message::Toolbar(crate::toolbar::Event::PpqEditConfirmed)
            );
            if !is_ppq_edit_msg {
                if let Ok(ppq) = self.toolbar.ppq_edit_buffer.parse::<u16>()
                    && (24..=32767).contains(&ppq)
                {
                    self.set_ppq(ppq);
                    tracing::info!("PPQ 已更新为 {}", ppq);
                }
                self.toolbar.ppq_editing = false;
                self.toolbar.ppq_edit_buffer.clear();
            }
        }

        if !self.try_handle_direct(&message) {
            let mut router = create_message_router();
            router.route(self, message);
        }
    }

    // ─── 子处理函数 ──────────────────────────────────────────────────────────

    /// 处理走带视图水平滚动
    fn handle_arrangement_scroll_x(&mut self, x: f32) -> bool {
        // 先计算缓存的最大 tick（可能扫描 track_notes），再借用 viewport
        let max_tick = self.arrangement_max_tick_end();
        let vp = &mut self.arrangement_view.viewport;
        let canvas_w = vp.canvas_size.x.max(1.0);
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

    /// 处理模式切换（编辑器 ↔ 瀑布流）
    fn handle_mode_toggle(&mut self) -> bool {
        use crate::sidebar::GroupId;
        use crate::titlebar::mode_toggle::AppMode;
        let target_mode = match self.state.current_mode {
            AppMode::Editor => AppMode::Waterfall,
            AppMode::Waterfall => AppMode::Editor,
        };
        if target_mode == AppMode::Waterfall {
            // 通过分组系统切换
            self.sidebar
                .update(crate::sidebar::Event::GroupToggled(GroupId::Waterfall));
        } else {
            // 从瀑布流转回 → 恢复钢琴卷帘组
            self.sidebar
                .update(crate::sidebar::Event::GroupToggled(GroupId::PianoRoll));
        }
        let target_progress = match target_mode {
            AppMode::Editor => 0.0,
            AppMode::Waterfall => 1.0,
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

        // 轮询异步 MoveOp 提交结果（每帧一次，将后台线程结果应用到 data 并 push history）
        if self.editor.poll_async_commit().is_some() {
            self.editor
                .invalidate_caches(crate::editor::CacheInvalidation::ALL);
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }

        // 清理过期 Toast（每帧调用，低成本 O(N) retain）
        self.toast.cleanup_expired(std::time::Instant::now());

        true
    }

    /// 处理简单的状态更新消息
    fn try_handle_simple_state(&mut self, msg: &Message) -> bool {
        match msg {
            Message::Progress(progress) => {
                if let Some((ref msg, p)) = *progress {
                    self.progress = Some((msg.clone(), p));
                } else {
                    self.progress = None;
                }
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
                self.editor
                    .set_canvas_offset(iced_core::Point::new(offset.x, offset.y));
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
                        let total_ticks = state.view.total_ticks;
                        lumino_core::editor_state::viewport::Viewport::new(
                            &mut state.view,
                            &mut state.max_scroll,
                        )
                        .update_max_scroll(total_ticks);
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
            Message::ToggleSettings | Message::Null => true,
            Message::ModeToggled => self.handle_mode_toggle(),
            Message::AnimationTick => self.handle_animation_tick(),
            Message::VelocityPanelResize(height) => {
                self.visual.velocity_panel_height = *height;
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
        use crate::titlebar::mode_toggle::AppMode;

        // 自动化面板切换始终触发重绘
        if matches!(&event, sidebar::Event::AutomationPanelToggled) {
            self.sidebar.update(event);
            return true;
        }

        // 钢琴卷帘切换始终触发重绘
        if matches!(&event, sidebar::Event::PianoRollToggled) {
            // 互斥：打开钢琴卷帘时退出瀑布流模式
            if !self.sidebar.piano_roll_visible {
                self.state.current_mode = AppMode::Editor;
                self.state.toggle_animation.animate_to(0.0);
            }
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
        let needs_redraw = self.sidebar.update(event.clone());

        // 分组切换 → 同步 AppMode（必须在 sidebar.update 之后，因为 active_group 在那里改变）
        if matches!(&event, sidebar::Event::GroupToggled(_)) {
            match self.sidebar.active_group {
                Some(sidebar::GroupId::Waterfall) => {
                    self.state.current_mode = AppMode::Waterfall;
                    self.state.toggle_animation.animate_to(1.0);
                }
                _ => {
                    self.state.current_mode = AppMode::Editor;
                    self.state.toggle_animation.animate_to(0.0);
                }
            }
        }

        // 音频导出面板打开时，从设置自动填充音色库路径（用户选择可覆盖）
        if matches!(
            &event,
            sidebar::Event::RouteUpdated(sidebar::Route::AudioExport)
        ) && self.sidebar.audio_export_visible
            && self.state.audio_export_dialog.soundfont_path.is_empty()
        {
            self.state.audio_export_dialog.soundfont_path = self.settings.soundfont_path.clone();
        }

        // 导出类路由 → 同步 sidebar 面板状态（已在 sidebar.update 中处理）
        // 音频/视频渲染面板状态由 sidebar.*_export_visible 驱动，view_main 中渲染

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

        // 如果是添加音轨，根据用户设置决定是否切换到新音轨
        if matches!(&event, sidebar::Event::AddTrack) {
            if self.settings.track_add_behavior == TrackAddBehavior::AutoSwitch {
                let track_idx = self.sidebar.tracks.last().map(|t| t.id).unwrap_or(0);
                self.sidebar.selected_track = track_idx;
                tracing::debug!("Root: 添加音轨后自动选中新音轨 {}", track_idx);
                crate::event::emit(crate::event::Event::Menu(crate::event::menu::Event::File(
                    crate::event::menu::file::Event::TrackSelected(track_idx),
                )));
            } else {
                tracing::debug!(
                    "Root: 添加音轨，保持当前音轨 {} 不变",
                    self.sidebar.selected_track
                );
            }
        }

        needs_redraw
    }

    /// 处理编辑器动作
    ///
    /// 返回 `true` 表示音符数据确实发生了变化。
    pub(crate) fn handle_editor_action(&mut self, action: EditorAction) -> bool {
        // 演奏指示线移动与滚动不修改音符数据，直接返回 false，
        // 避免被误判为脏音轨而触发昂贵的后台重生成。
        let is_playhead_or_scroll = matches!(
            action,
            EditorAction::Scrubbed { .. }
                | EditorAction::IndicatorDragStart { .. }
                | EditorAction::IndicatorDragMove { .. }
                | EditorAction::Scrolled { .. }
        );

        // 编辑拦截：Undo/Redo 在编辑状态下被 Editor::undo/redo 拦截，
        // 这里检测拦截并按 UiConfig 设置显示 Toast 提示用户。
        if matches!(action, EditorAction::Undo | EditorAction::Redo) && self.editor.is_editing() {
            if self.intercept_notification_enabled() {
                self.toast.push(
                    crate::toast::ToastLevel::Warning,
                    "请先完成当前编辑（拖动 / 绘制 / 调整大小）后再执行撤销/重做",
                );
            }
            tracing::debug!(
                "Editor: 拦截 {:?}（toast_enabled={}, edit_state={:?}）",
                action,
                self.intercept_notification_enabled(),
                self.editor.editor_state.interaction.edit_state
            );
            return false;
        }

        let old_tick = self.editor.playback_position;
        self.editor.handle_action(action);
        let new_tick = self.editor.playback_position;

        // 检查播放位置是否变化
        if (old_tick - new_tick).abs() > f32::EPSILON
            && let Some(manager) = &mut self.playback.manager
        {
            manager.seek(new_tick);
        }

        if is_playhead_or_scroll {
            return false;
        }

        // 检查音符数据是否变化
        let notes_changed = self.editor.notes_changed();
        if notes_changed {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
        notes_changed
    }

    /// 处理钢琴卷帘右键上下文菜单动作
    fn handle_piano_roll_context_menu(
        &mut self,
        action: lumino_message::PianoRollContextMenuAction,
    ) {
        use lumino_message::{PianoRollContextMenuAction, PianoRollContextMenuItem};

        match action {
            PianoRollContextMenuAction::Open { position } => {
                self.editor
                    .context_menu
                    .open(iced_core::Point::new(position.x, position.y));
            }
            PianoRollContextMenuAction::Close => {
                self.editor.context_menu.close();
            }
            PianoRollContextMenuAction::ItemClicked(item) => {
                self.editor.context_menu.close();
                let editor_action = match item {
                    PianoRollContextMenuItem::Cut => EditorAction::Cut,
                    PianoRollContextMenuItem::Copy => EditorAction::Copy,
                    PianoRollContextMenuItem::Paste => EditorAction::Paste,
                    PianoRollContextMenuItem::Delete => EditorAction::DeletePressed,
                    PianoRollContextMenuItem::SelectAll => EditorAction::SelectAll,
                };
                self.handle_editor_action(editor_action);
            }
        }
    }
}

#[cfg(test)]
mod handlers_tests;
