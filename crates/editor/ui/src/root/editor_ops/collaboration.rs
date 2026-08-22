//! 编辑器操作 - 协作功能

use crate::root::Root;
use crate::state::root_state::{CollaborationViewState, DialogType};

impl Root {
    /// 设置协作对话框是否打开
    pub fn set_collaboration_dialog_open(&mut self, open: bool) {
        self.state.collaboration_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::Collaboration;
            self.state.collaboration_dialog.view_state = CollaborationViewState::Connect;
            self.state.collaboration_dialog.connection_status = "未连接".to_string();
        }
        tracing::info!("协作对话框状态: {}", open);
    }

    /// 设置协作视图状态
    ///
    /// 返回视图状态是否发生变更（供 runner 决定是否广播），避免无谓的对话框刷新。
    pub fn set_collaboration_view_state(
        &mut self,
        state: CollaborationViewState,
        invite_code: Option<String>,
        room_name: Option<String>,
    ) -> bool {
        let changed = self.state.collaboration_dialog.view_state != state;
        self.state.collaboration_dialog.view_state = state;
        if let Some(code) = invite_code {
            self.state.collaboration_dialog.invite_code = code;
        }
        if let Some(name) = &room_name {
            self.state.collaboration_dialog.room_name = name.clone();
        }
        match state {
            CollaborationViewState::Connect => {
                // 连接失败时通过 room_name 参数透传具体原因（如"用户不存在"/"密码错误"）；
                // 否则保持默认"未连接"提示。
                self.state.collaboration_dialog.connection_status =
                    room_name.unwrap_or_else(|| "未连接".to_string());
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
        changed
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
        self.editor.update_remote_cursor(
            user_id.to_string().into(),
            position.x,
            position.y,
            color_str.into(),
            username.into(),
        );
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
            NoteAction::Add => self.handle_remote_notes_add(operation),
            NoteAction::Update => self.handle_remote_notes_update(operation),
            NoteAction::Delete => self.handle_remote_notes_delete(operation),
            NoteAction::Move => self.handle_remote_notes_move(operation),
            _ => {
                tracing::debug!("协作: 未处理的笔记操作类型: {:?}", operation.action);
            }
        }

        // 标记音符已变化，重建当前音轨的空间索引
        self.editor.mark_notes_changed();
    }

    fn handle_remote_notes_add(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        for note in &operation.notes {
            // 转换协作音符为编辑器音符
            let editor_note = crate::editor::note::Note::new(note.tick, note.key, note.length);

            // 2026-08 单一权威源：直接写入 document 指定音轨（track_notes 缓存已删除）
            let track_idx = note.track_index;
            self.editor
                .editor_state
                .data
                .insert_note(track_idx, editor_note);
        }
        // 精确标记受影响音轨（洋葱皮事件级增量）
        let affected: std::collections::HashSet<usize> =
            operation.notes.iter().map(|n| n.track_index).collect();
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
        // 音符由 wgpu 渲染，不需要清 grid cache
        tracing::info!("协作: 已添加 {} 个远程音符", operation.notes.len());
    }

    fn handle_remote_notes_update(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        // 更新操作：根据位置匹配现有音符
        for note in &operation.notes {
            let track_idx = note.track_index;
            // 2026-08 单一权威源：从 document 读取并匹配（track_notes 缓存已删除）
            let notes = self.editor.editor_state.data.track_notes(track_idx);
            let Some(match_idx) = notes.iter().position(|n| {
                (n.start_tick as f32 - note.tick).abs() < 1.0 && n.key as u16 == note.key
            }) else {
                continue;
            };
            // 保持其他字段不变，仅更新长度（NoteEvent 为 Copy，先取值再写回）
            let current = notes[match_idx];
            self.editor.editor_state.data.update_note(
                track_idx,
                match_idx,
                crate::editor::note::Note::from_raw(
                    current.start_tick as f32,
                    current.key as u16,
                    note.length,
                    current.velocity,
                    current.channel,
                ),
            );
        }
        // 精确标记受影响音轨（洋葱皮事件级增量）
        let affected: std::collections::HashSet<usize> =
            operation.notes.iter().map(|n| n.track_index).collect();
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
        // 音符由 wgpu 渲染，不需要清 grid cache
        tracing::info!("协作: 已更新 {} 个远程音符", operation.notes.len());
    }

    fn handle_remote_notes_delete(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        // 删除操作：根据位置匹配删除音符（索引从大到小删除，避免索引偏移）
        for note in &operation.notes {
            let track_idx = note.track_index;
            // 2026-08 单一权威源：从 document 读取并匹配（track_notes 缓存已删除）
            let notes = self.editor.editor_state.data.track_notes(track_idx);
            let mut match_indices: Vec<usize> = notes
                .iter()
                .enumerate()
                .filter(|(_, n)| {
                    (n.start_tick as f32 - note.tick).abs() < 1.0 && n.key as u16 == note.key
                })
                .map(|(i, _)| i)
                .collect();
            match_indices.sort_unstable_by(|a, b| b.cmp(a));
            for idx in match_indices {
                self.editor.editor_state.data.remove_note(track_idx, idx);
            }
        }
        // 精确标记受影响音轨（洋葱皮事件级增量）
        let affected: std::collections::HashSet<usize> =
            operation.notes.iter().map(|n| n.track_index).collect();
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
        // 音符由 wgpu 渲染，不需要清 grid cache
        tracing::info!("协作: 已删除 {} 个远程音符", operation.notes.len());
    }

    fn handle_remote_notes_move(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        let tick_offset = operation.tick_offset.unwrap_or(0.0);
        let key_offset = operation.key_offset.unwrap_or(0);
        tracing::debug!(
            "协作: Move 操作 - tick_offset={}, key_offset={}, notes数量={}, source_track={:?}",
            tick_offset,
            key_offset,
            operation.notes.len(),
            operation.source_track
        );
        let mut matched_count = 0;
        for note in &operation.notes {
            tracing::trace!(
                "协作: Move 查找音符 - target_tick={}, target_key={}, track={}",
                note.tick,
                note.key,
                note.track_index
            );
            // 2026-08 单一权威源：从 document 读取并匹配（track_notes 缓存已删除）
            let track_idx = note.track_index;
            let notes = self.editor.editor_state.data.track_notes(track_idx);
            let Some(match_idx) = notes.iter().position(|n| {
                (n.start_tick as f32 - note.tick).abs() < 1.0 && n.key as u16 == note.key
            }) else {
                tracing::warn!("协作: track {} 不存在或音符未匹配", track_idx);
                continue;
            };
            tracing::trace!(
                "协作:   [{}] tick={}, key={}",
                match_idx,
                notes[match_idx].start_tick,
                notes[match_idx].key
            );
            // NoteEvent 为 Copy：先取值再写回，避免借用冲突
            let current = notes[match_idx];
            let new_tick = current.start_tick as f32 + tick_offset;
            let new_key = (current.key as i16 + key_offset).max(0) as u16;
            tracing::debug!(
                "协作:   匹配成功! 更新: tick {} -> {}, key {} -> {}",
                current.start_tick,
                new_tick,
                current.key,
                new_key
            );
            self.editor.editor_state.data.update_note(
                track_idx,
                match_idx,
                crate::editor::note::Note::from_raw(
                    new_tick,
                    new_key,
                    current.length() as f32,
                    current.velocity,
                    current.channel,
                ),
            );
            matched_count += 1;
        }
        // 精确标记受影响音轨（洋葱皮事件级增量）
        let affected: std::collections::HashSet<usize> =
            operation.notes.iter().map(|n| n.track_index).collect();
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(affected));
        // 音符由 wgpu 渲染，不需要清 grid cache
        tracing::info!(
            "协作: Move 完成 - 匹配 {}/{} 个音符, current_track={}",
            matched_count,
            operation.notes.len(),
            self.editor.editor_state.data.current_track
        );
    }
}
