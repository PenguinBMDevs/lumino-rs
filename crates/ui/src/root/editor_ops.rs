//! Root 编辑器操作子模块

use crate::editor::note::Note;
use crate::root::Root;
use crate::state::root_state::{CollaborationViewState, DialogType};
use crate::toolbar;

impl Root {
    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<crate::message::AudioAction> {
        self.editor.take_audio_actions()
    }

    /// 更新编辑器鼠标位置
    pub fn update_editor_cursor(&mut self, position: Option<iced_core::Point>) {
        self.editor.update_cursor_position(position);
    }

    /// 更新编辑器 Canvas 偏移量
    pub fn set_editor_canvas_offset(&mut self, offset: iced_core::Point) {
        self.editor.set_canvas_offset(offset);
    }

    /// 设置菜单打开状态（菜单打开时不渲染预览音符）
    pub fn set_menu_open(&mut self, open: bool) {
        self.state.is_menu_open = open;
    }

    /// 获取当前是否应该渲染预览音符
    pub fn should_render_preview_note(&self) -> bool {
        !self.state.is_menu_open && !self.is_progress_window
    }

    /// 更新音轨列表（从 MIDI 导入）
    pub fn update_tracks(&mut self, track_infos: &[(usize, Option<String>, u64)]) {
        self.sidebar.update_tracks_from_midi(track_infos);
    }

    /// 设置编辑器总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: f32) {
        self.editor.state.total_ticks = total_ticks as u32;
        self.editor.max_scroll_x = total_ticks * self.editor.state.zoom_x;
    }

    /// 加载音符到编辑器
    pub fn load_notes(&mut self, notes: &[(f32, u8, f32)]) {
        self.editor.notes.clear();
        for (tick, key, length) in notes {
            // MIDI key (0-127) 映射到编辑器 key (0-127，反转顺序)
            let editor_key = *key as u16;
            self.editor
                .notes
                .push(Note::new(*tick, editor_key, *length));
        }
        // 清除网格缓存以强制重绘
        self.editor.grid_cache.clear();
    }

    /// 设置当前音轨
    pub fn set_current_track(&mut self, track_idx: usize) {
        self.sidebar.set_selected_track(track_idx);
        // 同时更新编辑器的当前音轨（用于无 MIDI 文件时的多音轨编辑）
        self.editor.switch_to_track(track_idx);
    }

    /// 加载指定音轨的音符到编辑器（用于 MIDI 文件）
    /// 这会同时更新当前显示的音符和音轨存储，以便洋葱皮能显示
    pub fn load_track_notes(&mut self, track_idx: usize, notes: &[(f32, u8, f32)]) {
        tracing::debug!(
            "Root::load_track_notes: track_idx={}, notes_count={}",
            track_idx,
            notes.len()
        );

        // 清空当前音符并加载新音符
        self.editor.notes.clear();
        let mut track_notes = Vec::with_capacity(notes.len());

        for (tick, key, length) in notes {
            let editor_key = *key as u16;
            let note = Note::new(*tick, editor_key, *length);
            self.editor.notes.push(note.clone());
            track_notes.push(note);
        }

        // 保存到 track_notes，供洋葱皮使用
        if !track_notes.is_empty() {
            self.editor.track_notes.insert(track_idx, track_notes);
            tracing::debug!(
                "Root::load_track_notes: saved {} notes to track_notes[{}]",
                notes.len(),
                track_idx
            );
        }

        // 更新当前音轨索引
        self.editor.current_track = track_idx;

        // 清除网格缓存以强制重绘
        self.editor.grid_cache.clear();
    }

    /// 设置自定义精度对话框是否打开
    pub fn set_custom_precision_dialog_open(&mut self, open: bool) {
        self.state.custom_precision_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::CustomPrecision;
        }
    }

    /// 获取并清空对话框结果
    pub fn take_dialog_result(&mut self) -> Option<crate::host::DialogResult> {
        self.state.dialog_result.take()
    }

    /// 设置自定义精度值
    pub fn set_custom_precision(&mut self, ticks: f32) {
        self.editor.state.snap_precision = ticks;
        self.editor.state.default_note_length = ticks;
        self.state.note_precision = toolbar::NotePrecision::Custom;
        tracing::info!("自定义精度已设置为 {} ticks", ticks);
    }

    /// 设置协作对话框是否打开
    pub fn set_collaboration_dialog_open(&mut self, open: bool) {
        self.state.collaboration_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::Collaboration;
            self.state.collaboration_dialog.view_state = CollaborationViewState::Connect;
        }
        tracing::info!("协作对话框状态: {}", open);
    }

    /// 设置协作视图状态
    pub fn set_collaboration_view_state(
        &mut self,
        state: CollaborationViewState,
        invite_code: Option<String>,
        room_name: Option<String>,
    ) {
        self.state.collaboration_dialog.view_state = state;
        if let Some(code) = invite_code {
            self.state.collaboration_dialog.invite_code = code;
        }
        if let Some(name) = room_name {
            self.state.collaboration_dialog.room_name = name;
        }
        match state {
            CollaborationViewState::Connect => {
                self.state.collaboration_dialog.connection_status = "未连接".to_string();
            }
            CollaborationViewState::Connecting => {
                self.state.collaboration_dialog.connection_status = "正在连接...".to_string();
            }
            CollaborationViewState::RoomActions => {
                self.state.collaboration_dialog.connection_status =
                    "已连接，请创建或加入房间".to_string();
            }
            CollaborationViewState::InRoom => {
                self.state.collaboration_dialog.connection_status = format!(
                    "房间: {} | 邀请码: {}",
                    self.state.collaboration_dialog.room_name,
                    self.state.collaboration_dialog.invite_code
                );
            }
        }
        tracing::info!("协作视图状态已更新: {:?}", state);
    }

    /// 更新远程光标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: u64,
        position: iced_core::Point,
        color: [f32; 4],
        username: String,
    ) {
        // 将颜色数组转换为十六进制字符串
        let color_str = format!(
            "{:02X}{:02X}{:02X}",
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8
        );
        self.editor
            .update_remote_cursor(user_id.to_string(), position, color_str, username);
    }

    /// 更新远程音符
    pub fn update_remote_note(&mut self, user_id: u64, operation: String) {
        // 这里将来可以解析 JSON 并应用到编辑器
        tracing::info!(
            "协作: 处理远端音符更新 - 用户: {}, 操作: {}",
            user_id,
            operation
        );
    }

    /// 应用远程笔记操作到本地编辑器
    pub fn apply_remote_note_operation(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        use lumino_collaboration::types::NoteAction;

        match operation.action {
            NoteAction::Add => {
                for note in &operation.notes {
                    // 转换协作音符为编辑器音符
                    let editor_note =
                        crate::editor::note::Note::new(note.tick, note.key, note.length);

                    // 添加到对应的音轨
                    let track_idx = note.track_index;
                    if track_idx == self.editor.current_track {
                        // 如果是当前音轨，直接添加到编辑器
                        self.editor.notes.push(editor_note.clone());
                    }

                    // 更新 track_notes
                    let track_notes = self
                        .editor
                        .track_notes
                        .entry(track_idx)
                        .or_insert_with(Vec::new);
                    track_notes.push(editor_note);
                }
                self.editor.grid_cache.clear();
                tracing::info!("协作: 已添加 {} 个远程音符", operation.notes.len());
            }
            NoteAction::Update => {
                // 更新操作：根据位置匹配现有音符
                for note in &operation.notes {
                    if let Some(track_notes) = self.editor.track_notes.get_mut(&note.track_index) {
                        for editor_note in track_notes.iter_mut() {
                            // 基于 tick 和 key 匹配（简化匹配）
                            if (editor_note.tick - note.tick).abs() < 1.0
                                && editor_note.key == note.key
                            {
                                editor_note.length = note.length;
                                break;
                            }
                        }
                    }
                }
                self.editor.grid_cache.clear();
                tracing::info!("协作: 已更新 {} 个远程音符", operation.notes.len());
            }
            NoteAction::Delete => {
                // 删除操作：根据位置匹配删除音符
                for note in &operation.notes {
                    if let Some(track_notes) = self.editor.track_notes.get_mut(&note.track_index) {
                        track_notes
                            .retain(|n| !((n.tick - note.tick).abs() < 1.0 && n.key == note.key));
                    }
                }
                // 同时更新当前显示的音符
                if let Some(source_track) = operation.source_track {
                    if source_track == self.editor.current_track {
                        self.editor.notes = self
                            .editor
                            .track_notes
                            .get(&source_track)
                            .cloned()
                            .unwrap_or_default();
                    }
                }
                self.editor.grid_cache.clear();
                tracing::info!("协作: 已删除 {} 个远程音符", operation.notes.len());
            }
            NoteAction::Move => {
                let tick_offset = operation.tick_offset.unwrap_or(0.0);
                let key_offset = operation.key_offset.unwrap_or(0);
                for note in &operation.notes {
                    if let Some(track_notes) = self.editor.track_notes.get_mut(&note.track_index) {
                        for editor_note in track_notes.iter_mut() {
                            // 根据原始 tick+key 匹配音符
                            if (editor_note.tick - note.tick).abs() < 1.0
                                && editor_note.key == note.key
                            {
                                editor_note.tick += tick_offset;
                                editor_note.key = (editor_note.key as i16 + key_offset).max(0) as u16;
                                break;
                            }
                        }
                    }
                }
                // 同时更新当前显示的音符
                if let Some(source_track) = operation.source_track {
                    if source_track == self.editor.current_track {
                        if let Some(track_notes) =
                            self.editor.track_notes.get(&source_track)
                        {
                            self.editor.notes = track_notes.clone();
                        }
                    }
                }
                self.editor.grid_cache.clear();
                tracing::info!("协作: 已移动 {} 个远程音符", operation.notes.len());
            }
            _ => {
                tracing::debug!("协作: 未处理的笔记操作类型: {:?}", operation.action);
            }
        }
    }
}
