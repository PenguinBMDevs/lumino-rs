//! 侧边栏事件与模式切换处理器
//!
//! 处理 `Message::Sidebar` 以及 `Message::ModeToggled`，
//! 负责侧边栏路由、音轨选择、分组切换与 AppMode 同步。

use crate::root::Root;
use crate::sidebar;
use crate::titlebar::mode_toggle::AppMode;
use lumino_core::storage::config::TrackAddBehavior;

impl Root {
    /// 处理模式切换（编辑器 ↔ 瀑布流）
    pub(crate) fn handle_mode_toggle(&mut self) -> bool {
        use crate::sidebar::GroupId;

        let target_mode = match self.state.current_mode {
            AppMode::Editor => AppMode::Waterfall,
            AppMode::Waterfall => AppMode::Editor,
        };
        if target_mode == AppMode::Waterfall {
            // 通过分组系统切换
            self.sidebar
                .update(sidebar::Event::GroupToggled(GroupId::Waterfall));
        } else {
            // 从瀑布流转回 → 恢复钢琴卷帘组
            self.sidebar
                .update(sidebar::Event::GroupToggled(GroupId::PianoRoll));
        }
        let target_progress = match target_mode {
            AppMode::Editor => 0.0,
            AppMode::Waterfall => 1.0,
        };
        self.state.current_mode = target_mode;
        self.state.toggle_animation.animate_to(target_progress);
        true
    }

    /// 处理侧边栏事件
    ///
    /// 返回是否需要重新渲染
    pub(crate) fn handle_sidebar_event(&mut self, event: sidebar::Event) -> bool {
        // 窗口最大化/还原期间阻止路由被意外切换
        if self.window_resize_guard
            && matches!(
                &event,
                sidebar::Event::RouteUpdated(_) | sidebar::Event::GroupToggled(_)
            )
        {
            tracing::warn!("Root: 窗口最大化/还原期间忽略路由切换");
            return false;
        }

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
        let mut needs_redraw = self.sidebar.update(event.clone());

        // 事件列表跳转：直接切换到目标位置
        if let sidebar::Event::EventListJump(ref req) = event {
            self.handle_event_list_jump(req);
            needs_redraw = true;
        }

        // 消费 sidebar 缓存的 editor 操作
        if let Some(action) = self.sidebar.take_event_list_action() {
            self.apply_event_list_action(action);
            needs_redraw = true;
        }

        // 解析需要 EditorData 访问的 popup / 编辑请求
        if let Some((req, value)) = self.sidebar.take_event_list_edit() {
            if let Some(action) = self.parse_event_list_edit(req, value) {
                self.apply_event_list_action(action);
            }
            needs_redraw = true;
        }

        // 消费 sidebar 中待删除音轨请求，构造 payload 转发给 Runner 写入 .lmdeltrack
        // 必须在 sidebar.update 之后调用——此时 pending_track_deletion 才被设置。
        self.forward_pending_track_deletion();

        // 消费 sidebar 中"找回删除音轨"对话框打开请求，转发给 Runner 打开对话框
        self.forward_pending_recover_track_dialog();

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
                let track_idx = self
                    .sidebar
                    .tracks
                    .last()
                    .map(|track| track.id)
                    .unwrap_or(0);
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
}

/// 处理右侧栏动作
impl Root {
    pub(crate) fn handle_right_sidebar_action(
        &mut self,
        action: lumino_message::RightSidebarAction,
    ) -> bool {
        use lumino_message::RightSidebarAction::*;
        match action {
            TogglePanel => {
                self.right_sidebar.toggle_panel();
                true
            }
            ImageToMidiClicked => {
                // 图片转MIDI功能将在这里处理，后续实现
                tracing::info!("右侧栏图片转MIDI按钮被点击");
                true
            }
            ResizeDragStarted => {
                // 拖拽开始由鼠标位置判断，这里只是标记开始
                self.right_sidebar.is_resizing = true;
                true
            }
            ResizeDragged => {
                // 拖拽中更新宽度
                true
            }
            ResizeDragEnded => {
                self.right_sidebar.end_resize();
                true
            }
        }
    }
}
