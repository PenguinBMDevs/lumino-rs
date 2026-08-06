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
            ImageToMidiClicked => {
                // 点击按钮展开/收起面板（面板展开方向向左），面板状态决定按钮亮灯
                self.right_sidebar.toggle_panel();
                tracing::info!(
                    "右侧栏图片转MIDI按钮被点击，面板{}",
                    if self.right_sidebar.panel_visible {
                        "展开"
                    } else {
                        "收起"
                    }
                );
                true
            }
            SelectImageFile => {
                // 面板内文件选择按钮：弹出对话框，让用户选择 i2m-rs 支持的图片文件
                // （PNG/JPEG/BMP/GIF/WebP/SVG），选中后标注路径。
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("选择要转换为 MIDI 的图片")
                    .add_filter(
                        "图片文件",
                        &["png", "jpg", "jpeg", "bmp", "gif", "webp", "svg"],
                    )
                    .add_filter("PNG 图片", &["png"])
                    .add_filter("JPEG 图片", &["jpg", "jpeg"])
                    .add_filter("BMP 图片", &["bmp"])
                    .add_filter("GIF 图片", &["gif"])
                    .add_filter("WebP 图片", &["webp"])
                    .add_filter("SVG 矢量图", &["svg"])
                    .add_filter("所有文件", &["*"])
                    .pick_file()
                {
                    self.right_sidebar.set_selected_image_path(path.clone());
                    tracing::info!("已选择图片转 MIDI 源文件: {}", path.display());
                }
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
            ConvertClicked => {
                // 面板内转换按钮：后台线程执行 i2m-rs 转换，
                // 完成后由 poll_pending_i2m 轮询接收并强制切换到 Y 向选择工具。
                let Some(path) = self.right_sidebar.selected_image_path.clone() else {
                    self.toast.push(
                        crate::toast::ToastLevel::Warning,
                        "请先选择图片文件再执行转换",
                    );
                    return true;
                };
                // 标记转换中：面板按钮禁用 + 编辑器进入等待框选阶段
                self.right_sidebar.converting = true;
                // 记录转换前的工具，√ 写入成功后还原
                self.i2m_restore_tool = Some(self.toolbar.current_tool);
                self.editor.editor_state.image_to_midi.begin_converting();
                // 后台线程执行转换，结果通过 channel 回传
                let thread_path = path.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let result = crate::right_sidebar::convert::run_conversion(&thread_path);
                    let _ = tx.send(result);
                });
                self.pending_i2m = Some(rx);
                tracing::info!("已启动图片转 MIDI 后台转换: {}", path.display());
                true
            }
            PlacementConfirm => {
                // 确认生成：调用 i2m-rs 逻辑在内存中写入数据（m8 实现）
                self.handle_i2m_placement_confirm();
                true
            }
            PlacementCancel => {
                // 取消生成：仅清除区域框（保留预览，可重新框选）
                self.editor.editor_state.image_to_midi.clear_region();
                self.right_sidebar.converting = false;
                self.editor
                    .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);
                tracing::info!("图片转 MIDI 放置已取消（保留预览，可重新框选）");
                true
            }
        }
    }

    /// 确认图片转 MIDI 生成：按逐轨写入/自动建轨策略写入 document
    ///
    /// - 颜色 0 写入当前音轨；
    /// - 颜色 1+ 自动创建新音轨（sidebar + document 同步扩轨）；
    /// - 使用 `CreateOp` 操作日志记录（跨轨撤销/重做）。
    fn handle_i2m_placement_confirm(&mut self) {
        use lumino_editor_state::ImageToMidiMode;

        // 快照放置状态（避免与后续 &mut self 借用冲突）
        let i2m = self.editor.editor_state.image_to_midi.clone();
        if i2m.mode != ImageToMidiMode::Placing {
            self.toast
                .push(crate::toast::ToastLevel::Warning, "请先框选生成区域");
            return;
        }
        let Some(preview) = &i2m.preview else {
            self.toast
                .push(crate::toast::ToastLevel::Warning, "没有可写入的转换数据");
            return;
        };

        let current_track = self.editor.editor_state.data.current_track;
        // 收集每轨音符（区域映射后的屏幕 tick/key/length）
        let mut tracks_data: Vec<Vec<(f32, u8, f32)>> = Vec::with_capacity(preview.tracks.len());
        let mut total_notes = 0usize;
        for (idx, _) in preview.tracks.iter().enumerate() {
            let notes = i2m.track_screen_notes(idx);
            total_notes += notes.len();
            tracks_data.push(notes);
        }
        if total_notes == 0 {
            self.toast.push(
                crate::toast::ToastLevel::Warning,
                "转换结果为空，未写入任何音符",
            );
            return;
        }

        // 自动建轨：颜色 1+ 各分配一条新音轨（sidebar + document 同步）
        let before: std::collections::HashSet<usize> =
            self.sidebar.tracks.iter().map(|t| t.id).collect();
        for _ in 0..preview.tracks.len().saturating_sub(1) {
            self.sidebar.update(sidebar::Event::AddTrack);
        }
        let new_track_ids: Vec<usize> = self
            .sidebar
            .tracks
            .iter()
            .filter(|t| !before.contains(&t.id))
            .map(|t| t.id)
            .collect();

        // 逐轨写入（颜色 0 → 当前轨，颜色 1+ → 新音轨）
        let mut create_ops: Vec<lumino_note_core::history::CreateOp> = Vec::new();
        let mut affected = std::collections::HashSet::new();
        for (color_idx, notes) in tracks_data.iter().enumerate() {
            if notes.is_empty() {
                continue;
            }
            let target_track = if color_idx == 0 {
                current_track
            } else {
                new_track_ids
                    .get(color_idx - 1)
                    .copied()
                    .unwrap_or(current_track)
            };
            if !self.editor.editor_state.data.ensure_track(target_track) {
                continue;
            }
            for &(tick, key, length) in notes {
                // 批量归一化：i2m 区域等比映射产生亚 tick 数值（如 12418.724），
                // 写入前统一 round 为整数 tick/长度——既保证 note_to_event 对
                // tick 与 tick+length 的 round 结果一致（长度不变形），也从源头
                // 消除非整数 tick（f32_to_tick 因此走快速路径，零日志、零阻塞）。
                let tick = tick.round();
                let length = length.round().max(1.0);
                let note = lumino_note_core::note::Note::new(tick, u16::from(key), length);
                let event = lumino_editor_state::note_to_event(note.clone());
                if self
                    .editor
                    .editor_state
                    .data
                    .insert_note(target_track, note)
                {
                    create_ops.push(lumino_note_core::history::CreateOp {
                        track_id: target_track as u32,
                        note: event,
                    });
                }
            }
            affected.insert(target_track);
        }

        // 历史记录（跨轨撤销）+ 标记变化（洋葱皮增量：明确受影响音轨）
        if !create_ops.is_empty() {
            self.editor
                .editor_state
                .data
                .history
                .push_note_create(create_ops);
            self.editor
                .editor_state
                .data
                .mark_track_notes_changed_for(Some(affected));
        }

        // 清除放置模式，还原显示区域
        self.editor.editor_state.image_to_midi.cancel();
        self.right_sidebar.converting = false;
        // 完全还原工具：切回转换前的工具（√ 写入成功后流程结束）
        if let Some(tool) = self.i2m_restore_tool.take() {
            self.toolbar.current_tool = tool;
            self.editor.set_tool(tool);
        }
        // 清理放置前残留的交互状态：写入改变了音符索引，残留的选中集合与
        // pending_drag_state 仍指向写入前的索引，保留会导致后续调整音符长度时
        // 触发批量 ResizingSelection（连带周围音符长度改变）或 ghost 误偏移。
        self.editor.editor_state.interaction.selected_notes.clear();
        self.editor.clear_pending_drag();
        self.editor.mark_notes_changed();
        self.update_playback_notes();
        self.editor.clear_notes_changed();
        self.editor
            .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);

        self.toast.push(
            crate::toast::ToastLevel::Success,
            format!("已生成 {total_notes} 个音符"),
        );
        tracing::info!("图片转 MIDI 写入完成：{} 个音符", total_notes);
    }

    /// 轮询图片转 MIDI 后台转换结果（每帧 / 每次消息路由时调用）
    ///
    /// 转换完成后：填充预览数据 → 强制切换到 Y 向选择工具进入放置模式。
    pub(crate) fn poll_pending_i2m(&mut self) {
        let rx = match self.pending_i2m.as_ref() {
            Some(rx) => rx,
            None => return,
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(_) => return, // Empty / Disconnected
        };
        self.pending_i2m = None;
        match result {
            Ok(preview) => {
                self.editor.editor_state.image_to_midi.set_preview(preview);
                self.right_sidebar.converting = false;
                // 强制切换到 Y 向选择工具，用户用其框选生成区域
                let tool = crate::toolbar::Tool::PointerYSelect;
                self.toolbar.current_tool = tool;
                self.editor.set_tool(tool);
                self.editor
                    .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);
                self.toast
                    .push(crate::toast::ToastLevel::Info, "转换完成：请框选生成区域");
                tracing::info!("图片转 MIDI 转换完成，已强制切换到 Y 向选择工具");
            }
            Err(err) => {
                self.editor.editor_state.image_to_midi.cancel();
                self.right_sidebar.converting = false;
                // 转换失败：流程结束，清除原工具记录
                self.i2m_restore_tool = None;
                self.toast.push(
                    crate::toast::ToastLevel::Error,
                    format!("图片转 MIDI 转换失败: {err}"),
                );
                tracing::error!("图片转 MIDI 转换失败: {err}");
            }
        }
    }
}
