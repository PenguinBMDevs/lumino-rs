//! 侧边栏事件与模式切换处理器
//!
//! 处理 `Message::Sidebar` 以及 `Message::ModeToggled`，
//! 负责侧边栏路由、音轨选择、分组切换与 AppMode 同步。
//!
//! 子模块组织（按职责拆分，保持本文件 < 400 行）：
//! - `right_sidebar`: 右侧栏动作处理（图片转 MIDI / 素材库）
//! - `i2m`: 图片转 MIDI 后台转换结果轮询

use crate::root::Root;
use crate::sidebar;
use crate::titlebar::mode_toggle::AppMode;
use lumino_core::storage::config::TrackAddBehavior;

mod i2m;
mod right_sidebar;

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
    /// 返回是否需要重新渲染（仅供调用方决策；`Root::update` 始终将
    /// Sidebar 消息视为已处理，不会因无需重绘而把消息继续路由）
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

        // 混音台浮动面板控制（开关/最大化/拖拽）：状态在 `Root.mixer_panel`，
        // 始终触发重绘。拖拽以左下为锚点：向右增大左内缩、向下减小下内缩；
        // 并夹紧在左侧栏（48px）之外、屏幕范围内。拖拽采用 `on_move` 相对坐标
        // 递推：`offset += (p - grab)`，使面板跟随光标（单次事件延迟）。
        if matches!(
            &event,
            sidebar::Event::MixerPanelToggled
                | sidebar::Event::MixerPanelMaximizeToggled
                | sidebar::Event::MixerPanelDragStarted
                | sidebar::Event::MixerPanelDragEnded
                | sidebar::Event::MixerPanelDragged(_, _)
                | sidebar::Event::MixerPanelMasterVolumeChanged(_)
                | sidebar::Event::MixerPanelScrolled(_)
        ) {
            match event {
                sidebar::Event::MixerPanelToggled => {
                    self.mixer_panel.open = !self.mixer_panel.open;
                }
                sidebar::Event::MixerPanelMaximizeToggled => {
                    self.mixer_panel.maximized = !self.mixer_panel.maximized;
                }
                sidebar::Event::MixerPanelDragStarted => {
                    self.mixer_panel.dragging = true;
                    self.mixer_panel.last_cursor = None;
                }
                sidebar::Event::MixerPanelDragEnded => {
                    self.mixer_panel.dragging = false;
                    self.mixer_panel.last_cursor = None;
                }
                sidebar::Event::MixerPanelDragged(px, py) if self.mixer_panel.dragging => {
                    // px/py 为全窗口覆盖层给出的绝对光标位置；以增量方式跟随，
                    // 使面板在光标离开标题栏/窗口范围时仍持续移动。
                    match self.mixer_panel.last_cursor {
                        None => self.mixer_panel.last_cursor = Some((px, py)),
                        Some((lx, ly)) => {
                            let (ox, oy) = self.mixer_panel.offset;
                            let nx = (ox + (px - lx)).clamp(48.0, 4000.0);
                            let ny = (oy - (py - ly)).clamp(0.0, 2000.0);
                            self.mixer_panel.offset = (nx, ny);
                            self.mixer_panel.last_cursor = Some((px, py));
                        }
                    }
                }
                // 主音量变化：更新状态并即时同步到播放引擎（全局缩放所有通道增益）。
                sidebar::Event::MixerPanelMasterVolumeChanged(v) => {
                    self.mixer_panel.master_volume = v.clamp(0, sidebar::MIXER_MAX_VOLUME);
                    self.update_playback_track_mix();
                }
                // 横向滚动：仅更新裁剪偏移，触发重绘（视口节流在 build_body 生效）。
                sidebar::Event::MixerPanelScrolled(x) => {
                    self.mixer_panel.scroll_x = x;
                }
                _ => {}
            }
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

        // 静音/独奏切换 → 立即同步到播放引擎，使独奏/静音实时过滤播放。
        if matches!(
            &event,
            sidebar::Event::TrackMuteToggled(_) | sidebar::Event::TrackSoloToggled(_)
        ) {
            self.update_playback_track_states();
        }

        // 增益/声像变化 → 立即同步到播放引擎（音频域混音，与 MIDI CC 解耦）。
        if matches!(
            &event,
            sidebar::Event::TrackGainChanged(_, _) | sidebar::Event::TrackPanChanged(_, _)
        ) {
            self.update_playback_track_mix();
        }

        // 音轨新增类事件：同步扩展 document（单一权威源）。
        // 2026-08 修复：sidebar 新建音轨只更新 UI 列表，document 未扩轨，
        // 导致新音轨 insert_note 越界静默失败（音符无法放置）。
        // 必须在发射 TrackSelected 之前完成，保证切换后立即可编辑。
        if matches!(
            &event,
            sidebar::Event::AddTrack
                | sidebar::Event::TrackAddAbove(_)
                | sidebar::Event::TrackAddBelow(_)
        ) {
            self.ensure_sidebar_tracks_in_document();
        }

        // 音轨结构变化（拖拽排序 / 新增 / 删除）→ 同步视觉位置映射。
        // sidebar.tracks 顺序即视觉顺序，track_visual_order 是走带交互层
        // 把视觉位置转换为 document 音轨索引的依据，不同步会导致排序后
        // 走带编辑（添加/框选/移动/擦除/切割）落在错误的音轨上。
        let track_structure_changed = matches!(
            &event,
            sidebar::Event::AddTrack
                | sidebar::Event::TrackAddAbove(_)
                | sidebar::Event::TrackAddBelow(_)
                | sidebar::Event::TrackReorderEnded(_)
                | sidebar::Event::TrackContextMenuItemClicked(
                    _,
                    lumino_message::TrackContextMenuItem::Delete
                )
        ) || self.sidebar.pending_track_deletion.is_some();
        if track_structure_changed {
            self.sync_track_visual_order();
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
            self.state.audio_export_dialog.soundfont_path =
                self.settings.synth.soundfont_path.clone();
        }

        // 卷帘方向切换 → 镜像到右侧栏，驱动瀑布流预览入口按钮显隐
        // （纵向卷帘模式下该入口隐藏，因其内容已并入纵向卷帘编辑区）
        self.right_sidebar.roll_mode = self
            .sidebar
            .roll_bar_active
            .unwrap_or(sidebar::RollBarButton::Horizontal);

        // 卷帘方向 → 写入 editor_state，供自动滚动轴向与播放指示线方向共享同一事实源
        let was_vertical = self.editor.editor_state.is_vertical_roll;
        let is_vertical = self.sidebar.is_vertical_roll();
        self.editor.editor_state.is_vertical_roll = is_vertical;
        if is_vertical && !was_vertical {
            // 进入纵向：先备份横向视图，再重置键盘缩放以完整显示 128/256 键（铺满视口宽度）
            self.editor.editor_state.save_horizontal_backup();
            let vw = self.editor.editor_state.canvas.size_x;
            if vw > 1.0 {
                self.editor.fit_vertical_keyboard_to_viewport();
            } else {
                // 视口尚未初始化（首次进入），按典型宽度估算保证 128 键大致铺满
                let visible = self.editor.editor_state.view.visible_key_count as f32;
                let typical_vw = 1200.0;
                let target_zoom = (typical_vw / visible).clamp(
                    crate::constants::editor::zoom::MIN_ZOOM_Y,
                    crate::constants::editor::zoom::MAX_ZOOM_Y,
                );
                self.editor.editor_state.view.zoom_y = target_zoom;
                self.editor.editor_state.max_scroll.1 = visible * target_zoom;
                self.editor.editor_state.view.scroll_y = 0.0;
                self.editor.editor_state.view.smooth_scroll.target_y = 0.0;
                self.editor.editor_state.view.smooth_scroll.active = false;
                self.editor
                    .invalidate_caches(crate::editor::CacheInvalidation::KEYBOARD);
            }
        } else if !is_vertical && was_vertical {
            // 退出纵向：恢复横向视图备份，避免“音符消失”（缩放/滚动错位）
            self.editor.editor_state.restore_horizontal_backup();
            self.editor
                .invalidate_caches(crate::editor::CacheInvalidation::KEYBOARD);
            self.editor
                .invalidate_caches(crate::editor::CacheInvalidation::RULER);
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
            if self.settings.editing.track_add_behavior == TrackAddBehavior::AutoSwitch {
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
