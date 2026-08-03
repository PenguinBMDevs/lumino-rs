//! 音轨删除 / 恢复类窗口事件处理
//!
//! UI 通过 `window::track::Event` 把音轨删除 / 恢复请求传给 Runner：
//! - `DeleteTrack`：sidebar 已删除入口，Runner 负责把音轨数据写入 `.lmdeltrack` 缓存
//! - `RestoreTrack` / `PermanentlyDeleteTrack`：用户点击对话框按钮后的磁盘 I/O
//!   （通常走 `DialogResult` 路径，事件通道作为备用入口）
//! - `RecoverTrackDialogScanned` / `TrackRestored` / `TrackPermanentlyDeleted`：
//!   Runner → UI 方向事件，Runner 收到时为异常场景，仅 warn 日志

use std::path::PathBuf;

use crate::runner::RunnerInner;
use lumino_ui::event::window::track::Event;
use lumino_ui::event::window::track::RecoverTrackEntryPayload;

impl RunnerInner {
    pub(crate) fn handle_track_events(&mut self, window_event: Event) {
        match window_event {
            Event::DeleteTrack(payload) => {
                self.handle_delete_track(payload);
            }
            Event::RestoreTrack {
                path,
                original_index,
            } => {
                self.handle_restore_track(path, original_index);
            }
            Event::PermanentlyDeleteTrack { path, track_id } => {
                self.handle_permanently_delete_track(path, track_id);
            }
            // Runner → UI 方向事件，不应由 Runner 处理
            Event::RecoverTrackDialogScanned(_) => {
                tracing::warn!(
                    "handle_track_events: 收到 Runner→UI 方向的 RecoverTrackDialogScanned，已忽略"
                );
            }
            Event::TrackRestored(_) => {
                tracing::warn!("handle_track_events: 收到 Runner→UI 方向的 TrackRestored，已忽略");
            }
            Event::TrackPermanentlyDeleted { track_id } => {
                tracing::warn!(
                    "handle_track_events: 收到 Runner→UI 方向的 TrackPermanentlyDeleted track_id={}，已忽略",
                    track_id
                );
            }
        }
    }

    /// 处理 DeleteTrack：把被删除音轨的数据写入 `.lmdeltrack` 缓存文件
    ///
    /// UI 已从 sidebar.tracks 中移除入口并标记 track_id 为 reserved，
    /// Runner 负责把音轨元数据 + 音符列表持久化到磁盘。
    fn handle_delete_track(
        &mut self,
        payload: lumino_ui::event::window::track::TrackDeletionPayload,
    ) {
        let cache_dir = self.deleted_track_cache_dir();
        let note_count = payload.notes.len() as u64;

        // 构造 lumino_project 的元数据结构
        let meta = lumino_project::DeletedTrackMetadata {
            track_id: payload.track_id,
            track_name: payload.track_name.clone(),
            port: payload.port,
            channel: payload.channel,
            note_count,
            deleted_at: now_iso8601(),
            original_index: payload.original_index,
            is_drum: payload.is_drum,
            max_tick: payload.max_tick,
        };

        // 构造 lumino_project 的音符数据结构
        let data = lumino_project::DeletedTrackData {
            notes: payload
                .notes
                .iter()
                .map(|n| lumino_project::DeletedNote {
                    start_tick: n.start_tick,
                    end_tick: n.end_tick,
                    key: n.key,
                    velocity: n.velocity,
                    channel: n.channel,
                    port: n.port,
                })
                .collect(),
        };

        match lumino_project::save_deleted_track(&cache_dir, &meta, &data) {
            Ok(path) => {
                tracing::info!(
                    "Runner: 已删除音轨缓存写入成功 track_id={} notes={} → {}",
                    payload.track_id,
                    note_count,
                    path.display()
                );
            }
            Err(e) => {
                tracing::error!(
                    "Runner: 已删除音轨缓存写入失败 track_id={} err={}",
                    payload.track_id,
                    e
                );
            }
        }
    }

    /// 处理 RestoreTrack：从 `.lmdeltrack` 加载音轨数据，回填到主窗口 UI
    ///
    /// 此路径为事件通道备用入口，对话框"恢复"按钮通常走 `DialogResult::RecoverTrackRestore`。
    fn handle_restore_track(&mut self, path: PathBuf, original_index: usize) {
        let path_ref = &path;
        match lumino_project::load_deleted_track(path_ref) {
            Ok((meta, data)) => {
                let payload = build_payload_from_deleted(meta, data, original_index);
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_track_restored(payload);
                tracing::info!("Runner: 已从缓存恢复音轨 path={}", path.display());
            }
            Err(e) => {
                tracing::error!(
                    "Runner: 加载已删除音轨缓存失败 path={} err={}",
                    path.display(),
                    e
                );
            }
        }
    }

    /// 处理 PermanentlyDeleteTrack：销毁 `.lmdeltrack` 文件并释放 reserved track_id
    ///
    /// 此路径为事件通道备用入口，对话框"永久删除"按钮通常走
    /// `DialogResult::RecoverTrackPermanentlyDelete`。
    fn handle_permanently_delete_track(&mut self, path: PathBuf, track_id: u16) {
        match lumino_project::delete_permanently(&path) {
            Ok(()) => {
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_track_permanently_deleted(track_id);
                tracing::info!(
                    "Runner: 已永久销毁音轨缓存 path={} track_id={}",
                    path.display(),
                    track_id
                );
            }
            Err(e) => {
                tracing::error!(
                    "Runner: 永久销毁音轨缓存失败 path={} err={}",
                    path.display(),
                    e
                );
            }
        }
    }

    /// 计算已删除音轨缓存目录
    ///
    /// 优先使用工程目录下的 `.lumino/deleted_tracks/`（跟随工程移动）；
    /// 若当前未打开工程，回退到全局配置目录下的 `deleted_tracks/`。
    pub(crate) fn deleted_track_cache_dir(&self) -> PathBuf {
        if let Some(ref midi_source) = self.midi_state.current_midi_source
            && let Some(parent) = midi_source.parent()
        {
            return parent.join(".lumino").join("deleted_tracks");
        }
        crate::storage::config_dir().join("deleted_tracks")
    }

    /// 扫描缓存目录，构造对话框条目 payload 列表
    ///
    /// 返回 `(entries, cache_dir)`：entries 为条目列表，cache_dir 为实际扫描的目录
    /// （用于日志）。扫描失败时返回空列表而非报错，保持对话框可用。
    pub(crate) fn scan_recover_track_entries(&self) -> Vec<RecoverTrackEntryPayload> {
        let cache_dir = self.deleted_track_cache_dir();
        match lumino_project::list_deleted_tracks(&cache_dir) {
            Ok(entries) => entries
                .into_iter()
                .map(|e| RecoverTrackEntryPayload {
                    path: e.path,
                    filename: e.filename,
                    track_id: e.metadata.track_id,
                    track_name: e.metadata.track_name,
                    port: e.metadata.port,
                    channel: e.metadata.channel,
                    note_count: e.metadata.note_count,
                    deleted_at: e.metadata.deleted_at,
                    original_index: e.metadata.original_index,
                })
                .collect(),
            Err(e) => {
                tracing::warn!(
                    "Runner: 扫描已删除音轨缓存目录失败 dir={} err={}",
                    cache_dir.display(),
                    e
                );
                Vec::new()
            }
        }
    }

    /// 尝试把 pending 的条目列表填充到就绪的 RecoverTrack 对话框
    ///
    /// 在 `about_to_wait` 中每帧调用：
    /// - 若 `pending_recover_track_entries` 有值且对话框 UI 就绪 → 填充并清空 pending
    /// - 若对话框尚未就绪 → 保留 pending，下一帧重试
    /// - 若没有 pending → 立即返回
    pub(crate) fn try_fill_recover_track_entries(&mut self) {
        let entries = match self.pending_recover_track_entries.take() {
            Some(e) => e,
            None => return,
        };

        // 查找就绪的 RecoverTrack 对话框
        let dialog_id = self
            .window_state
            .dialog_manager
            .first_dialog_id_of_type(lumino_ui::state::root_state::DialogType::RecoverTrack);

        match dialog_id {
            Some(id) => {
                if let Some(dialog) = self.window_state.dialog_manager.get_dialog_mut(id)
                    && let Some(ui) = dialog.ui_mut()
                {
                    ui.apply_recover_track_entries(entries.clone());
                    dialog.request_redraw();
                    tracing::debug!(
                        "Runner: 已填充找回删除音轨对话框条目 count={}",
                        entries.len()
                    );
                }
            }
            None => {
                // 对话框尚未就绪，保留 pending 下一帧重试
                self.pending_recover_track_entries = Some(entries);
            }
        }
    }
}

/// 从 lumino_project 的 (meta, data) 构造 UI 用的 TrackDeletionPayload
fn build_payload_from_deleted(
    meta: lumino_project::DeletedTrackMetadata,
    data: lumino_project::DeletedTrackData,
    original_index: usize,
) -> lumino_ui::event::window::track::TrackDeletionPayload {
    let notes = data
        .notes
        .into_iter()
        .map(|n| lumino_ui::event::window::track::TrackDeletionNote {
            start_tick: n.start_tick,
            end_tick: n.end_tick,
            key: n.key,
            velocity: n.velocity,
            channel: n.channel,
            port: n.port,
        })
        .collect();

    lumino_ui::event::window::track::TrackDeletionPayload {
        track_id: meta.track_id,
        track_name: meta.track_name,
        port: meta.port,
        channel: meta.channel,
        is_drum: meta.is_drum,
        max_tick: meta.max_tick,
        original_index,
        notes,
    }
}

/// 生成当前时间的 ISO 8601 字符串（用于 deleted_at 字段）
fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("ts:{secs}")
}
