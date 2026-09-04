//! Runner 协作处理

use crate::runner::inner::LastSentMouse;
use crate::runner::{CollaborationStatus, RunnerInner};
use lumino_ui::state::root_state::CollaborationViewState;

/// 本地音符快照（`handle_local_note_added/deleted` 共用 7 字段，结构体化满足 `too_many_arguments`）。
pub(super) struct LocalNoteSnapshot {
    /// 音符全局唯一 ID
    pub id: u64,
    /// 起始 tick
    pub tick: f32,
    /// 音高
    pub key: u16,
    /// 长度
    pub length: f32,
    /// 力度
    pub velocity: u8,
    /// 通道
    pub channel: u8,
    /// 音轨索引
    pub track_index: usize,
}

/// 本地音符移动事件（7 字段，结构体化满足 `too_many_arguments`）。
pub(super) struct LocalNoteMove {
    /// 音符全局唯一 ID
    pub id: u64,
    /// 移动后起始 tick
    pub tick: f32,
    /// 音高
    pub key: u16,
    /// 长度
    pub length: f32,
    /// tick 偏移
    pub tick_offset: f32,
    /// key 偏移
    pub key_offset: i16,
    /// 音轨索引
    pub track_index: usize,
}

impl RunnerInner {
    /// 同步协作状态（发送鼠标位置等）
    ///
    /// 采用变更检测：仅当内容坐标、滚动或缩放相对上次发送发生可感知变化时才入队，
    /// 避免每 50ms 无脑发送造成日志洪泛与带宽浪费。
    pub(super) fn sync_collaboration_state(&mut self) {
        // 检查是否已连接
        let is_connected = self.collab_state.collaboration_service.is_connected();
        if !is_connected {
            return;
        }

        // 获取最新的鼠标位置（从 Host 而不是 Editor）
        let cursor_pos = self.window_state.window.ui().cursor_position();
        let editor = self.window_state.window.ui().root().editor_ref();

        let es = &editor.editor_state;

        if let Some(pos) = cursor_pos {
            // 先转换为 Canvas 视口坐标（不含滚动偏移），用于边界检查
            let viewport_pos =
                iced_core::Point::new(pos.x - es.canvas.offset_x, pos.y - es.canvas.offset_y);

            if editor.is_inside_canvas(viewport_pos) {
                // 通过边界检查后，加上滚动偏移得到内容空间坐标
                let content_pos = iced_core::Point::new(
                    viewport_pos.x + es.view.scroll_x,
                    viewport_pos.y + es.view.scroll_y,
                );

                let scroll_x = es.view.scroll_x;
                let scroll_y = es.view.scroll_y;
                let zoom_x = es.view.zoom_x;
                let zoom_y = es.view.zoom_y;

                // 变更检测：与上次发送快照比较（坐标/滚动/缩放），epsilon = 0.01
                let changed = match self.collab_state.last_sent_mouse {
                    None => true,
                    Some(prev) => {
                        (prev.x - content_pos.x).abs() > 0.01
                            || (prev.y - content_pos.y).abs() > 0.01
                            || (prev.scroll_x - scroll_x).abs() > 0.01
                            || (prev.scroll_y - scroll_y).abs() > 0.01
                            || (prev.zoom_x - zoom_x).abs() > 0.01
                            || (prev.zoom_y - zoom_y).abs() > 0.01
                    }
                };

                if !changed {
                    return;
                }

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

                if let Err(e) = self
                    .collab_state
                    .collaboration_service
                    .send_mouse_position(mouse_pos)
                {
                    tracing::debug!("协作：发送鼠标位置失败：{}", e);
                    // 发送失败（如连接已断开）时清除快照，下次成功后再记录
                    self.collab_state.last_sent_mouse = None;
                    return;
                }

                self.collab_state.last_sent_mouse = Some(LastSentMouse {
                    x: content_pos.x,
                    y: content_pos.y,
                    scroll_x,
                    scroll_y,
                    zoom_x,
                    zoom_y,
                });
            } else {
                // 光标移出画布：清空快照，移回画布时立即重新发送首帧
                self.collab_state.last_sent_mouse = None;
            }
        }
    }

    /// 处理协作连接
    pub(crate) fn handle_collaboration_connect(
        &mut self,
        host: String,
        port: u16,
        username: String,
        password: String,
        room_name: Option<String>,
        invite_code: Option<String>,
    ) {
        // 更新状态为连接中，并广播到协作对话框（若存在）
        self.collab_state.collaboration_status = CollaborationStatus::Connecting;
        // 记录服务器地址（工程文件同步 HTTP 请求用）
        self.collab_state.server_host = host.clone();
        self.collab_state.server_port = port;
        self.set_main_collab_view_state(CollaborationViewState::Connecting, None, None);

        // 使用协作服务连接
        let service = self.collab_state.collaboration_service.clone();
        tokio::spawn(async move {
            if let Err(e) = service
                .connect(host, port, username, password, room_name, invite_code)
                .await
            {
                tracing::error!("协作连接失败: {}", e);
            }
        });
    }

    /// 将协作视图状态写入主窗口 Root，并广播到所有已打开的协作对话框。
    ///
    /// 主窗口 Root 是协作状态的唯一数据源；对话框为独立 Root，需通过
    /// `DialogManager::forward_collaboration_view_state` 同步最新视图状态，
    /// 否则对话框永远停在“连接中”而无法进入房间。
    /// 仅广播视图状态与房间信息，**排除连接表单字段**。返回是否发生了实际变更。
    pub(crate) fn set_main_collab_view_state(
        &mut self,
        state: CollaborationViewState,
        invite_code: Option<String>,
        room_name: Option<String>,
    ) -> bool {
        let changed = self
            .window_state
            .window
            .ui_mut()
            .set_collaboration_view_state(state, invite_code.clone(), room_name.clone());

        self.window_state
            .dialog_manager
            .forward_collaboration_view_state(state, invite_code, room_name);

        changed
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
        if let Err(e) = self.collab_state.collaboration_service.disconnect() {
            tracing::error!("协作: 断开连接失败: {}", e);
        }
    }

    /// 处理本地笔记添加（同步到其他用户）
    pub(super) fn handle_local_note_added(&self, note: LocalNoteSnapshot) {
        let LocalNoteSnapshot {
            id,
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        } = note;
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }

        let location = NoteLocation {
            tick,
            key,
            track_index,
            channel,
        };
        let modifiers = NoteOperationModifiers {
            length,
            velocity,
            target_track: Some(track_index),
            tick_offset: None,
            key_offset: None,
        };
        let operation = build_sync_note_operation(
            lumino_collaboration::types::NoteAction::Add,
            id,
            &location,
            &modifiers,
        );

        if let Err(e) = self
            .collab_state
            .collaboration_service
            .send_note_batch(operation)
        {
            tracing::debug!("协作: 发送笔记添加失败: {}", e);
        } else {
            tracing::info!("协作: 已发送笔记添加 - tick={}, key={}", tick, key);
        }
    }

    /// 处理本地批量音符添加（100K 粘贴，分片发送避免单帧过大）
    pub(super) fn handle_local_notes_added_batch(
        &self,
        notes: Vec<(u64, f32, u16, f32, u8, u8, usize)>,
    ) {
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }
        if notes.is_empty() {
            return;
        }
        // 分片：每 10K 一条 NoteBatchOperation，避免单条 JSON 过大（WebSocket 帧限制）
        const CHUNK: usize = 10_000;
        for chunk in notes.chunks(CHUNK) {
            let sync_notes: Vec<lumino_collaboration::types::SyncNote> = chunk
                .iter()
                .map(|(id, tick, key, length, velocity, channel, track_index)| {
                    lumino_collaboration::types::SyncNote {
                        id: *id,
                        tick: *tick,
                        key: *key,
                        length: *length,
                        velocity: *velocity,
                        channel: *channel,
                        track_index: *track_index,
                    }
                })
                .collect();
            let operation = lumino_collaboration::types::NoteBatchOperation {
                action: lumino_collaboration::types::NoteAction::Add,
                notes: sync_notes,
                source_track: chunk.first().map(|(_, _, _, _, _, _, t)| *t),
                target_track: chunk.first().map(|(_, _, _, _, _, _, t)| *t),
                tick_offset: None,
                key_offset: None,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };
            if let Err(e) = self
                .collab_state
                .collaboration_service
                .send_note_batch(operation)
            {
                tracing::debug!("协作: 发送批量添加失败: {}", e);
                break;
            }
        }
        tracing::info!("协作: 已发送批量添加 - 总数 {}", notes.len());
    }

    /// 处理本地音符移动（同步到其他用户）
    pub(super) fn handle_local_note_moved(&self, mv: LocalNoteMove) {
        let LocalNoteMove {
            id,
            tick,
            key,
            length,
            tick_offset,
            key_offset,
            track_index,
        } = mv;
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }

        let location = NoteLocation {
            tick,
            key,
            track_index,
            channel: 0,
        };
        let modifiers = NoteOperationModifiers {
            length,
            velocity: 100,
            target_track: Some(track_index),
            tick_offset: Some(tick_offset),
            key_offset: Some(key_offset),
        };
        let operation = build_sync_note_operation(
            lumino_collaboration::types::NoteAction::Move,
            id,
            &location,
            &modifiers,
        );

        if let Err(e) = self
            .collab_state
            .collaboration_service
            .send_note_batch(operation)
        {
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

    /// 处理本地音符删除（同步到其他用户）
    pub(super) fn handle_local_note_deleted(&self, note: LocalNoteSnapshot) {
        let LocalNoteSnapshot {
            id,
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        } = note;
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }

        let location = NoteLocation {
            tick,
            key,
            track_index,
            channel,
        };
        let modifiers = NoteOperationModifiers {
            length,
            velocity,
            target_track: None,
            tick_offset: None,
            key_offset: None,
        };
        let operation = build_sync_note_operation(
            lumino_collaboration::types::NoteAction::Delete,
            id,
            &location,
            &modifiers,
        );

        if let Err(e) = self
            .collab_state
            .collaboration_service
            .send_note_batch(operation)
        {
            tracing::debug!("协作: 发送音符删除失败: {}", e);
        } else {
            tracing::info!("协作: 已发送音符删除 - tick={}, key={}", tick, key);
        }
    }

    /// 处理本地音轨添加（同步到其他用户）
    pub(super) fn handle_local_track_added(&self, track_index: usize) {
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }

        let update = lumino_collaboration::types::ProjectUpdate {
            update_type: lumino_collaboration::types::ProjectUpdateType::Track,
            data: serde_json::json!({
                "action": "add",
                "trackIndex": track_index,
            }),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        if let Err(e) = self
            .collab_state
            .collaboration_service
            .send_project_update(update)
        {
            tracing::debug!("协作: 发送音轨添加失败: {}", e);
        } else {
            tracing::info!("协作: 已发送音轨添加 - track_index={}", track_index);
        }
    }

    /// 处理本地选择变更（同步到其他用户）
    ///
    /// 构造 `{active, timestamp, fingerprints}` JSON 并经协作通道广播。
    pub(super) fn handle_local_selection_changed(
        &self,
        active: bool,
        timestamp: u64,
        fingerprints: Vec<[f64; 4]>,
    ) {
        if !self.collab_state.collaboration_service.is_connected() {
            return;
        }

        let selection = serde_json::json!({
            "active": active,
            "timestamp": timestamp,
            "fingerprints": fingerprints,
        });

        if let Err(e) = self
            .collab_state
            .collaboration_service
            .send_selection(selection)
        {
            tracing::debug!("协作: 发送选择变更失败: {}", e);
        } else {
            tracing::debug!(
                "协作: 已发送选择变更 - active={}, timestamp={}, 指纹数={}",
                active,
                timestamp,
                fingerprints.len()
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
        self.window_state
            .window
            .ui_mut()
            .apply_remote_note_operation(&operation);
    }

    /// 处理远程工程更新（来自其他用户，如音轨变更）
    pub(super) fn handle_remote_project_update(&mut self, user_id: String, update_json: String) {
        tracing::info!("协作: 处理远程工程更新 - 用户: {}", user_id);

        let update: lumino_collaboration::types::ProjectUpdate =
            match serde_json::from_str(&update_json) {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!("协作: 解析工程更新失败: {}", e);
                    return;
                }
            };

        match update.update_type {
            lumino_collaboration::types::ProjectUpdateType::Track => {
                if let Some(action) = update.data.get("action").and_then(|v| v.as_str())
                    && action == "add"
                {
                    let track_idx = update
                        .data
                        .get("trackIndex")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    tracing::info!("协作: 远程添加音轨 - track_index={}", track_idx);

                    self.window_state
                        .window
                        .ui_mut()
                        .add_remote_track(track_idx);
                    self.window_state.window.window().request_redraw();
                }
            }
            _ => {
                tracing::debug!("协作: 未处理的工程更新类型: {:?}", update.update_type);
            }
        }
    }
}

// ── Helper functions ─────────────────────────────────────────────────

/// 音符定位信息。
///
/// 用于聚合标识一个音符在工程中的位置与所属音轨/通道。
#[derive(Debug, Clone, Copy)]
struct NoteLocation {
    /// 时间刻度（tick）
    tick: f32,
    /// 音高键位
    key: u16,
    /// 所属音轨索引
    track_index: usize,
    /// MIDI 通道
    channel: u8,
}

/// 音符操作修饰参数。
///
/// 用于聚合构建 `NoteBatchOperation` 时补充的时长、力度、目标音轨及偏移信息。
#[derive(Debug, Clone, Copy)]
struct NoteOperationModifiers {
    /// 音符长度
    length: f32,
    /// 音符力度
    velocity: u8,
    /// 目标音轨（移动操作时使用）
    target_track: Option<usize>,
    /// 时间偏移（移动操作时使用）
    tick_offset: Option<f32>,
    /// 键位偏移（移动操作时使用）
    key_offset: Option<i16>,
}

/// 根据操作类型、真实音符 ID 与修饰参数构建同步操作。
///
/// `note_id` 为发送端文档分配的全局唯一音符 ID（来自 `LocalNoteAdded/Moved/Deleted`
/// 事件透传），取代原先基于时间戳的伪 ID，使对端能按 id 精确匹配同一音符，
/// 并避免不同客户端分配器之间的 id 碰撞。
fn build_sync_note_operation(
    action: lumino_collaboration::types::NoteAction,
    note_id: u64,
    location: &NoteLocation,
    modifiers: &NoteOperationModifiers,
) -> lumino_collaboration::types::NoteBatchOperation {
    let note = lumino_collaboration::types::SyncNote {
        id: note_id,
        tick: location.tick,
        key: location.key,
        length: modifiers.length,
        velocity: modifiers.velocity,
        channel: location.channel,
        track_index: location.track_index,
    };

    lumino_collaboration::types::NoteBatchOperation {
        action,
        notes: vec![note],
        source_track: Some(location.track_index),
        target_track: modifiers.target_track,
        tick_offset: modifiers.tick_offset,
        key_offset: modifiers.key_offset,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    }
}
