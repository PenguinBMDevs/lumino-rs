//! 协作类窗口事件处理

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;

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
    /// 把工程序列化为 `.lmpj` 字节（纯数据操作，可在后台线程执行，不触碰 UI）。
    ///
    /// 接收已克隆出来的 `MidiDocument` 与 tempo 点，避免在主线程直接读取编辑器状态
    /// 导致大工程卡死 UI。返回 None 表示无工程或序列化失败。
    fn serialize_room_project(doc: &lumino_midi_loader::MidiDocument, tempo: &[(f32, f64)]) -> Option<Vec<u8>> {
        let mut project = lumino_export::LuminoProject::from_midi_document(doc);
        project.apply_tempo_points(tempo.iter().copied());

        let tmp_dir = storage::config_dir().join("user-data").join("rooms");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let path = tmp_dir.join(format!(
            "room_proj_{}.lmpj",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        if let Err(e) = lumino_export::save_to_archive(&project, &path) {
            tracing::warn!("协作: 序列化房间工程为归档失败，无法上传: {e}");
            return None;
        }
        let bytes = std::fs::read(&path).ok();
        let _ = std::fs::remove_file(&path);
        bytes
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

                // host 路径：上传当前工程到房间。
                // 关键修复：在 UI 线程仅 Clone 可 Send 的 MidiDocument/tempo（轻量），
                // 真正的序列化与上传搬到右台线程，避免大工程卡死主线程、进度条晚出。
                let code = invite_code.clone();
                let host = self.collab_state.server_host.clone();
                let port = self.collab_state.server_port;
                let name = if project_name.is_empty() {
                    room_name.clone()
                } else {
                    project_name
                };
                let (doc, tempo) = {
                    let ui = self.window_state.window.ui();
                    let data = &ui.root().editor.editor_state.data;
                    (
                        data.document.clone(),
                        data.tempo_points
                            .iter()
                            .map(|tp| (tp.tick, tp.bpm))
                            .collect::<Vec<_>>(),
                    )
                };
                let progress_cb = Arc::clone(&self.window_state.progress_cb);
                // 先弹进度对话框，立即给用户反馈（此时尚未开始序列化）
                progress_cb(&format!("正在准备协作工程 {name}…"), 0.0);
                tokio::spawn(async move {
                    let name_p = name.clone();
                    let name_for_progress = name.clone();
                    let cb_for_stream = Arc::clone(&progress_cb);
                    let on_progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>> =
                        Some(Arc::new(move |sent, total| {
                            let p = if total > 0 {
                                sent as f64 / total as f64
                            } else {
                                0.0
                            };
                            cb_for_stream(
                                &format!("正在上传协作工程 {}（{:.0}%）", name_for_progress, p * 100.0),
                                p,
                            );
                        }));
                    // 后台线程完成重活：序列化工程为 .lmpj 字节（doc 为 Option，需解包后传入）
                    let bytes = match doc {
                        Some(doc_inner) => match tokio::task::spawn_blocking({
                            let tempo = tempo.clone();
                            move || Self::serialize_room_project(&doc_inner, &tempo)
                        })
                        .await
                        {
                        Ok(b) => b,
                        Err(e) => {
                            progress_cb(&format!("生成协作工程失败：{e}"), 1.0);
                            return;
                        }
                        },
                        None => None,
                    };
                    let Some(bytes) = bytes else {
                        progress_cb("无工程可上传或生成失败", 1.0);
                        return;
                    };
                    let hash = hash_bytes(&bytes);
                    // 进度切到「上传中」，进入带百分比的流式上传
                    progress_cb(&format!("正在上传协作工程 {name_p}…"), 0.0);
                    let client = HttpClient::new(&host, port);
                    match client
                        .upload_room_project(&code, &name_p, &hash, bytes, on_progress)
                        .await
                    {
                        Ok(_) => {
                            progress_cb("协作工程上传完成", 1.0);
                            tracing::info!("协作: 已上传房间工程 (hash={hash})")
                        }
                        Err(e) => {
                            progress_cb(&format!("协作工程上传失败：{e}"), 1.0);
                            tracing::warn!("协作: 上传房间工程失败: {e}")
                        }
                    }
                });
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

                // joiner 路径：比对工程哈希，不一致则下载并打开 host 工程。
                // 关键修复：本地工程哈希比对也搬到后台线程（原 current_project_hash 在主线程同步
                // 序列化大工程会卡死 UI），并弹出进度对话框避免「接收时未响应」的观感。
                if project_hash.is_empty() {
                    tracing::warn!("协作: 房间无工程哈希，跳过下载");
                    return;
                }
                let (doc, tempo) = {
                    let ui = self.window_state.window.ui();
                    let data = &ui.root().editor.editor_state.data;
                    (
                        data.document.clone(),
                        data.tempo_points
                            .iter()
                            .map(|tp| (tp.tick, tp.bpm))
                            .collect::<Vec<_>>(),
                    )
                };
                let progress_cb = Arc::clone(&self.window_state.progress_cb);
                progress_cb(&format!("正在同步协作工程 {invite_code}…"), 0.0);
                let code = invite_code.clone();
                let host = self.collab_state.server_host.clone();
                let port = self.collab_state.server_port;
                tokio::spawn(async move {
                    // 后台计算本地工程哈希，与房间哈希比对（避免主线程序列化卡顿）
                    let local_hash = match doc {
                        Some(doc_inner) => tokio::task::spawn_blocking({
                            let tempo = tempo.clone();
                            move || {
                                Self::serialize_room_project(&doc_inner, &tempo)
                                    .map(|b| hash_bytes(&b))
                            }
                        })
                        .await
                        .ok()
                        .flatten(),
                        None => None,
                    };
                    if let Some(local) = local_hash {
                        if local == project_hash {
                            progress_cb("本地工程已是最新", 1.0);
                            tracing::info!("协作: 本地工程与房间一致，跳过下载");
                            return;
                        }
                    }

                    let client = HttpClient::new(&host, port);
                    progress_cb(&format!("正在下载协作工程 {code}…"), 0.0);
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
                                    progress_cb("协作工程已加载", 1.0);
                                    tracing::info!("协作: 已加载房间工程");
                                }
                                Ok(Err(e)) => {
                                    progress_cb(&format!("解析下载工程失败：{e}"), 1.0);
                                    tracing::warn!("协作: 解析下载工程失败: {e}")
                                }
                                Err(e) => {
                                    progress_cb(&format!("加载工程任务失败：{e}"), 1.0);
                                    tracing::warn!("协作: 加载工程任务失败: {e}")
                                }
                            }
                        }
                        Err(e) => {
                            progress_cb(&format!("下载房间工程失败：{e}"), 1.0);
                            tracing::warn!("协作: 下载房间工程失败: {e}")
                        }
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
