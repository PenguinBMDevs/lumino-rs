//! 协作类窗口事件处理

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::runner::RunnerInner;
use crate::storage;
use lumino_collaboration::http::HttpClient;
use lumino_export;
use lumino_project::project::load::load_project_from_bytes;
use lumino_ui::event::window::collaboration::Event;
use lumino_ui::state::root_state::CollaborationViewState;

/// 计算字节的 hex 哈希（用于工程一致性比对）
fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

impl RunnerInner {
    /// 构建当前打开工程的 `.lmpj` 字节（无工程时返回 None）
    fn build_current_project_bytes(&self) -> Option<Vec<u8>> {
        let ui = self.window_state.window.ui();
        let data = &ui.root().editor.editor_state.data;
        let doc = data.document.as_ref()?;
        let mut project = lumino_export::LuminoProject::from_midi_document(doc);
        project.apply_tempo_points(data.tempo_points.iter().map(|tp| (tp.tick, tp.bpm)));

        let tmp_dir = storage::config_dir().join("user-data").join("rooms");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let path = tmp_dir.join(format!("room_proj_{}.lmpj", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()));
        if lumino_export::save_to_archive(&project, &path).is_err() {
            return None;
        }
        let bytes = std::fs::read(&path).ok();
        let _ = std::fs::remove_file(&path);
        bytes
    }

    /// 计算当前打开工程的哈希（无工程时 None）
    fn current_project_hash(&self) -> Option<String> {
        self.build_current_project_bytes()
            .map(|b| hash_bytes(&b))
    }

    pub(crate) fn handle_collaboration_events(&mut self, window_event: Event) {
        use lumino_ui::event::window::collaboration::Event::*;
        match window_event {
            Connect {
                host,
                port,
                username,
                password,
                invite_code,
            } => {
                tracing::info!("请求连接协作服务器: {host}:{port}");
                self.handle_collaboration_connect(
                    host,
                    port,
                    username,
                    password,
                    None,
                    invite_code,
                );
            }
            CreateRoom { name } => {
                tracing::info!("请求创建协作房间: {name}");
                self.handle_collaboration_create_room(name);
            }
            JoinRoom { invite_code } => {
                tracing::info!("请求加入协作房间: {invite_code}");
                self.handle_collaboration_join_room(invite_code);
            }
            Disconnect => {
                tracing::info!("请求断开协作连接");
                self.handle_collaboration_disconnect();
            }
            Authenticated {
                user_id,
                invite_code,
            } => {
                tracing::info!("协作认证成功: user={user_id}, invite={invite_code}");
                // 认证成功：保持连接中（即将进入房间）
                self.set_main_collab_view_state(CollaborationViewState::Connecting, None, None);
            }
            RoomCreated {
                room_name,
                invite_code,
                project_name,
                project_hash: _,
            } => {
                tracing::info!("协作房间创建成功: {room_name}, invite={invite_code}");
                self.set_main_collab_view_state(
                    CollaborationViewState::InRoom,
                    Some(invite_code.clone()),
                    Some(room_name.clone()),
                );

                // host 路径：上传当前工程到房间（异步、非阻塞）
                let code = invite_code.clone();
                let host = self.collab_state.server_host.clone();
                let port = self.collab_state.server_port;
                let name = if project_name.is_empty() {
                    room_name.clone()
                } else {
                    project_name
                };
                if let Some(bytes) = self.build_current_project_bytes() {
                    let hash = hash_bytes(&bytes);
                    tokio::spawn(async move {
                        let client = HttpClient::new(&host, port);
                        match client.upload_room_project(&code, &name, &hash, bytes).await {
                            Ok(_) => {
                                tracing::info!("协作: 已上传房间工程 (hash={hash})")
                            }
                            Err(e) => tracing::debug!("协作: 上传房间工程失败: {e}"),
                        }
                    });
                } else {
                    tracing::debug!("协作: 当前无工程可上传");
                }
            }
            RoomJoined {
                room_name,
                invite_code,
                user_count,
                project_name: _,
                project_hash,
            } => {
                tracing::info!(
                    "已加入协作房间: {room_name}, invite={invite_code}, 用户数={user_count}"
                );
                self.set_main_collab_view_state(
                    CollaborationViewState::InRoom,
                    Some(invite_code.clone()),
                    Some(room_name.clone()),
                );

                // joiner 路径：比对工程哈希，不一致则下载并打开 host 工程
                if project_hash.is_empty() {
                    tracing::debug!("协作: 房间无工程哈希，跳过下载");
                    return;
                }
                let need_download = match self.current_project_hash() {
                    Some(local) => local != project_hash,
                    None => true, // 本地无工程，必须下载
                };
                if !need_download {
                    tracing::info!("协作: 本地工程与房间一致，跳过下载");
                    return;
                }

                let code = invite_code.clone();
                let host = self.collab_state.server_host.clone();
                let port = self.collab_state.server_port;
                tokio::spawn(async move {
                    let client = HttpClient::new(&host, port);
                    match client.download_room_project(&code).await {
                        Ok(bytes) => {
                            let load = tokio::task::spawn_blocking(move || {
                                let project = load_project_from_bytes(&bytes)
                                    .map_err(|e| e.to_string())?;
                                lumino_export::project_to_parsed_midi(
                                    &project,
                                    Path::new("collab_room_project.lmpj"),
                                )
                                .map_err(|e| e.to_string())
                            });
                            match load.await {
                                Ok(Ok(parsed)) => {
                                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                                        lumino_ui::event::menu::file::Event::MidiParsed(
                                            std::sync::Arc::new(parsed),
                                        ),
                                    ));
                                    tracing::info!("协作: 已加载房间工程");
                                }
                                Ok(Err(e)) => {
                                    tracing::debug!("协作: 解析下载工程失败: {e}")
                                }
                                Err(e) => tracing::debug!("协作: 加载工程任务失败: {e}"),
                            }
                        }
                        Err(e) => tracing::debug!("协作: 下载房间工程失败: {e}"),
                    }
                });
            }
            Disconnected => {
                tracing::info!("协作连接已断开");
                // 回到可连接态，允许重试
                self.set_main_collab_view_state(CollaborationViewState::Connect, None, None);
            }
            ConnectFailed { reason } => {
                tracing::error!("协作连接失败: {reason}");
                // 连接失败：回到可连接态并展示原因，允许重试
                self.set_main_collab_view_state(
                    CollaborationViewState::Connect,
                    None,
                    Some(reason),
                );
            }
            UserLeft { user_id } => {
                // 从主窗口与协作对话框移除远端光标
                self.window_state
                    .window
                    .ui_mut()
                    .remove_remote_cursor(user_id.clone());
                self.window_state
                    .dialog_manager
                    .forward_collaboration_user_left(user_id);
            }
            MouseUpdate {
                user_id,
                x,
                y,
                color,
                username,
            } => {
                tracing::trace!("协作鼠标更新: user={user_id}, ({x:.0},{y:.0})");
                // 更新主窗口远端光标（编辑器画布渲染）
                self.window_state.window.ui_mut().update_remote_cursor(
                    user_id.clone(),
                    x,
                    y,
                    color.clone(),
                    username.clone(),
                );
                // 同步到协作对话框（对话框亦展示远端光标）
                self.window_state
                    .dialog_manager
                    .forward_collaboration_cursor(user_id, x, y, color, username);
            }
            NoteUpdate { user_id, operation } => {
                self.handle_remote_note_update(user_id, operation);
            }
            ProjectUpdate { user_id, update } => {
                self.handle_remote_project_update(user_id, update);
            }
            Selection {
                user_id,
                selection,
                color,
            } => {
                tracing::debug!("协作: 应用远端选择高亮 - 用户: {user_id}");
                self.window_state
                    .window
                    .ui_mut()
                    .apply_remote_selection(user_id, selection, color);
            }
        }
    }
}
