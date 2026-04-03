//! Runner 协作处理

use crate::runner::{CollaborationStatus, RunnerInner};

impl RunnerInner {
    /// 同步协作状态（发送鼠标位置等）
    pub(super) fn sync_collaboration_state(&mut self) {
        // 检查是否已连接
        let is_connected = self.collaboration_service.is_connected();
        if !is_connected {
            return;
        }

        // 获取最新的鼠标位置（从 Host 而不是 Editor）
        let cursor_pos = self.window.ui().cursor_position();
        let editor = self.window.ui().root().editor_ref();

        tracing::debug!(
            "协作同步：cursor_position={:?}, canvas_offset=({}, {}), scroll=({}, {})",
            cursor_pos,
            editor.canvas_offset.x,
            editor.canvas_offset.y,
            editor.state.scroll_x,
            editor.state.scroll_y
        );

        if let Some(pos) = cursor_pos {
            // 先转换为 Canvas 视口坐标（不含滚动偏移），用于边界检查
            let viewport_pos = iced_core::Point::new(
                pos.x - editor.canvas_offset.x,
                pos.y - editor.canvas_offset.y,
            );

            if editor.is_inside_canvas(viewport_pos) {
                // 通过边界检查后，加上滚动偏移得到内容空间坐标
                let content_pos = iced_core::Point::new(
                    viewport_pos.x + editor.state.scroll_x,
                    viewport_pos.y + editor.state.scroll_y,
                );

                let scroll_x = editor.state.scroll_x;
                let scroll_y = editor.state.scroll_y;
                let zoom_x = editor.state.zoom_x;
                let zoom_y = editor.state.zoom_y;

                let mouse_pos = lumino_collaboration::types::MousePosition {
                    x: content_pos.x,
                    y: content_pos.y,
                    view_state: Some(lumino_collaboration::types::ViewState {
                        scroll_x,
                        scroll_y,
                        zoom_x,
                        zoom_y,
                        ..Default::default()
                    }),
                };

                tracing::debug!(
                    "协作：发送鼠标位置：x={}, y={}",
                    content_pos.x,
                    content_pos.y
                );

                if let Err(e) = self.collaboration_service.send_mouse_position(mouse_pos) {
                    tracing::debug!("协作：发送鼠标位置失败：{}", e);
                }
            }
        }
    }

    /// 处理协作连接
    pub(crate) fn handle_collaboration_connect(
        &mut self,
        host: String,
        port: u16,
        username: String,
        room_name: Option<String>,
        invite_code: Option<String>,
    ) {
        // 更新状态为连接中
        self.collaboration_status = CollaborationStatus::Connecting;

        // 使用协作服务连接
        let service = self.collaboration_service.clone();
        tokio::spawn(async move {
            if let Err(e) = service
                .connect(host, port, username, room_name, invite_code)
                .await
            {
                tracing::error!("协作连接失败: {}", e);
            }
        });
    }

    /// 处理创建房间
    pub(super) fn handle_collaboration_create_room(&self, name: String) {
        tracing::info!("协作: 请求创建房间 - {} (已转发到 UI 层)", name);
    }

    /// 处理加入房间
    pub(super) fn handle_collaboration_join_room(&self, invite_code: String) {
        tracing::info!("协作: 请求加入房间 - {} (已转发到 UI 层)", invite_code);
    }

    /// 处理断开连接
    pub(super) fn handle_collaboration_disconnect(&mut self) {
        tracing::info!("协作: 请求断开连接");
        if let Err(e) = self.collaboration_service.disconnect() {
            tracing::error!("协作: 断开连接失败: {}", e);
        }
    }

    /// 处理本地笔记添加（同步到其他用户）
    pub(super) fn handle_local_note_added(
        &self,
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    ) {
        // 检查是否已连接
        if !self.collaboration_service.is_connected() {
            return;
        }

        // 生成唯一ID
        let note_id = format!(
            "note_{}_{}_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            tick as u64,
            key,
            track_index
        );

        let note = lumino_collaboration::types::Note {
            id: note_id,
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        };

        let operation = lumino_collaboration::types::NoteBatchOperation {
            action: lumino_collaboration::types::NoteAction::Add,
            notes: vec![note],
            source_track: Some(track_index),
            target_track: Some(track_index),
            tick_offset: None,
            key_offset: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        if let Err(e) = self.collaboration_service.send_note_batch(operation) {
            tracing::debug!("协作: 发送笔记添加失败: {}", e);
        } else {
            tracing::info!("协作: 已发送笔记添加 - tick={}, key={}", tick, key);
        }
    }

    /// 处理本地音符移动（同步到其他用户）
    pub(super) fn handle_local_note_moved(
        &self,
        tick: f32,
        key: u16,
        length: f32,
        tick_offset: f32,
        key_offset: i16,
        track_index: usize,
    ) {
        if !self.collaboration_service.is_connected() {
            return;
        }

        // 生成唯一ID
        let note_id = format!(
            "note_move_{}_{}_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            tick as u64,
            key,
            track_index
        );

        let note = lumino_collaboration::types::Note {
            id: note_id,
            tick,
            key,
            length,
            velocity: 100,
            channel: 0,
            track_index,
        };

        let operation = lumino_collaboration::types::NoteBatchOperation {
            action: lumino_collaboration::types::NoteAction::Move,
            notes: vec![note],
            source_track: Some(track_index),
            target_track: Some(track_index),
            tick_offset: Some(tick_offset),
            key_offset: Some(key_offset),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        if let Err(e) = self.collaboration_service.send_note_batch(operation) {
            tracing::debug!("协作: 发送音符移动失败: {}", e);
        } else {
            tracing::info!(
                "协作: 已发送音符移动 - tick={}, key={}, offset=({}, {})",
                tick,
                key,
                tick_offset,
                key_offset
            );
        }
    }

    /// 处理远程笔记更新
    pub(super) fn handle_remote_note_update(&mut self, user_id: String, operation: String) {
        tracing::info!("协作: 处理远程笔记更新 - 用户: {}", user_id);

        // 解析操作
        let operation: lumino_collaboration::types::NoteBatchOperation =
            match serde_json::from_str(&operation) {
                Ok(op) => op,
                Err(e) => {
                    tracing::error!("协作: 解析笔记操作失败: {}", e);
                    return;
                }
            };

        // 应用到编辑器
        self.window.ui_mut().apply_remote_note_operation(&operation);
    }
}
