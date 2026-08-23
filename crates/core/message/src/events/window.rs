pub mod audio;
pub mod collaboration;
pub mod dialog;
pub mod lifecycle;
pub mod sync;
pub mod track;
pub mod video;

use std::sync::Arc;

// Dialog variant 已 Box 化以减小 Event 枚举体积（同 Message 布局评审结论）。
// const fn 构造函数经由 const helper `Event::dialog()` 使用 `Box::new`（Rust ≥1.85 支持）。
#[derive(Debug, Clone)]
/// 窗口事件
pub enum Event {
    /// 窗口生命周期事件
    Lifecycle(lifecycle::Event),
    /// 对话框事件
    Dialog(Box<dialog::Event>),
    /// 协作事件
    Collaboration(collaboration::Event),
    /// 同步事件
    Sync(sync::Event),
    /// 音轨删除 / 恢复（与 .lmdeltrack 缓存交互）
    Track(track::Event),
}

impl Event {
    // ── 对话框构造函数 ──

    /// 构造对话框事件（Box 化以减小 `Event` / `Message` 枚举体积）。
    ///
    /// 注意：`Box::new` 在稳定版 Rust 中暂非 const，故本函数非 `const fn`；
    /// 依赖它的对话框构造函数同样为非 const（无实际 const 用途）。
    pub fn dialog(ev: dialog::Event) -> Self {
        Self::Dialog(Box::new(ev))
    }

    // ── 生命周期构造函数（直接构造，无需中间函数） ──

    /// 构造拖拽窗口事件
    pub const fn drag() -> Self {
        Self::Lifecycle(lifecycle::Event::Drag)
    }
    /// 构造关闭窗口事件
    pub const fn close() -> Self {
        Self::Lifecycle(lifecycle::Event::Close)
    }
    /// 构造切换最大化事件
    pub const fn toggle_maximize() -> Self {
        Self::Lifecycle(lifecycle::Event::ToggleMaximize)
    }
    /// 构造最大化事件
    pub const fn maximize() -> Self {
        Self::Lifecycle(lifecycle::Event::Maximize)
    }
    /// 构造最小化事件
    pub const fn minimize() -> Self {
        Self::Lifecycle(lifecycle::Event::Minimize)
    }

    /// 构造打开自定义精度对话框事件
    pub fn open_custom_precision_dialog() -> Self {
        Self::dialog(dialog::Event::OpenCustomPrecisionDialog)
    }
    /// 构造关闭自定义精度对话框事件
    pub fn close_custom_precision_dialog() -> Self {
        Self::dialog(dialog::Event::CloseCustomPrecisionDialog)
    }
    /// 构造应用自定义精度事件
    pub fn apply_custom_precision(numerator: u32, denominator: u32) -> Self {
        Self::dialog(dialog::Event::ApplyCustomPrecision(numerator, denominator))
    }
    /// 构造打开加载确认对话框事件
    pub fn open_load_confirm_dialog(path: String, size_mb: f64) -> Self {
        Self::dialog(dialog::Event::OpenLoadConfirmDialog { path, size_mb })
    }
    /// 构造打开协作对话框事件
    pub fn open_collaboration_dialog() -> Self {
        Self::dialog(dialog::Event::OpenCollaborationDialog)
    }
    /// 构造关闭协作对话框事件
    pub fn close_collaboration_dialog() -> Self {
        Self::dialog(dialog::Event::CloseCollaborationDialog)
    }
    /// 构造打开变速对话框事件
    pub fn open_speed_change_dialog() -> Self {
        Self::dialog(dialog::Event::OpenSpeedChangeDialog)
    }
    /// 构造关闭变速对话框事件
    pub fn close_speed_change_dialog() -> Self {
        Self::dialog(dialog::Event::CloseSpeedChangeDialog)
    }
    /// 构造确认变速事件
    pub fn confirm_speed_change(factor: f32) -> Self {
        Self::dialog(dialog::Event::ConfirmSpeedChange(factor))
    }
    /// 构造打开批量编辑对话框事件
    pub fn open_batch_edit_dialog() -> Self {
        Self::dialog(dialog::Event::OpenBatchEditDialog)
    }
    /// 构造关闭批量编辑对话框事件
    pub fn close_batch_edit_dialog() -> Self {
        Self::dialog(dialog::Event::CloseBatchEditDialog)
    }
    /// 构造确认批量编辑事件
    pub fn confirm_batch_edit(velocity: String, gate: String, key: String, tick: String) -> Self {
        Self::dialog(dialog::Event::ConfirmBatchEdit {
            velocity,
            gate,
            key,
            tick,
        })
    }
    /// 构造打开视频导出对话框事件
    pub fn open_video_export_dialog() -> Self {
        Self::dialog(dialog::Event::OpenVideoExportDialog)
    }
    /// 构造关闭视频导出对话框事件
    pub fn close_video_export_dialog() -> Self {
        Self::dialog(dialog::Event::CloseVideoExportDialog)
    }
    /// 构造打开工程设置对话框事件
    pub fn open_project_settings_dialog() -> Self {
        Self::dialog(dialog::Event::OpenProjectSettingsDialog)
    }
    /// 构造关闭工程设置对话框事件
    pub fn close_project_settings_dialog() -> Self {
        Self::dialog(dialog::Event::CloseProjectSettingsDialog)
    }
    /// 构造打开内存监控对话框事件
    pub fn open_memory_monitor_dialog() -> Self {
        Self::dialog(dialog::Event::OpenMemoryMonitorDialog)
    }
    /// 构造关闭内存监控对话框事件
    pub fn close_memory_monitor_dialog() -> Self {
        Self::dialog(dialog::Event::CloseMemoryMonitorDialog)
    }
    /// 构造打开找回音轨对话框事件
    pub fn open_recover_track_dialog() -> Self {
        Self::dialog(dialog::Event::OpenRecoverTrackDialog)
    }
    /// 构造关闭找回音轨对话框事件
    pub fn close_recover_track_dialog() -> Self {
        Self::dialog(dialog::Event::CloseRecoverTrackDialog)
    }
    /// 构造删除音轨事件
    pub fn delete_track(payload: track::TrackDeletionPayload) -> Self {
        Self::Track(track::Event::DeleteTrack(payload))
    }
    /// 构造恢复音轨事件
    pub fn restore_track(path: std::path::PathBuf, original_index: usize) -> Self {
        Self::Track(track::Event::RestoreTrack {
            path,
            original_index,
        })
    }
    /// 构造永久删除音轨事件
    pub fn permanently_delete_track(path: std::path::PathBuf, track_id: u16) -> Self {
        Self::Track(track::Event::PermanentlyDeleteTrack { path, track_id })
    }
    /// 构造找回音轨对话框扫描完成事件
    pub fn recover_track_dialog_scanned(entries: Vec<track::RecoverTrackEntryPayload>) -> Self {
        Self::Track(track::Event::RecoverTrackDialogScanned(entries))
    }
    /// 构造音轨已恢复事件
    pub fn track_restored(payload: track::TrackDeletionPayload) -> Self {
        Self::Track(track::Event::TrackRestored(payload))
    }
    /// 构造音轨已永久删除事件
    pub fn track_permanently_deleted(track_id: u16) -> Self {
        Self::Track(track::Event::TrackPermanentlyDeleted { track_id })
    }
    /// 构造应用工程设置事件
    pub fn apply_project_settings(
        title: String,
        tempo: f64,
        copyright: String,
        author: String,
        time_signatures: Vec<(u32, u8, u8)>,
    ) -> Self {
        Self::dialog(dialog::Event::ApplyProjectSettings {
            title,
            tempo,
            copyright,
            author,
            time_signatures,
        })
    }
    /// 构造开始音频导出事件
    pub fn start_audio_export(
        config: dialog::AudioExportConfig,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) -> Self {
        Self::dialog(dialog::Event::StartAudioExport {
            config: Box::new(config),
            document,
        })
    }
    /// 构造开始视频导出事件
    pub fn start_video_export(
        config: dialog::VideoExportConfig,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) -> Self {
        Self::dialog(dialog::Event::StartVideoExport {
            config: Box::new(config),
            document,
        })
    }

    // ── 协作构造函数（直接构造 collaboration::Event） ──

    /// 构造协作连接事件
    pub fn collaboration_connect(
        host: String,
        port: u16,
        username: String,
        password: String,
        invite_code: Option<String>,
    ) -> Self {
        Self::Collaboration(collaboration::Event::Connect {
            host,
            port,
            username,
            password,
            invite_code,
        })
    }
    /// 构造协作创建房间事件
    pub fn collaboration_create_room(name: String) -> Self {
        Self::Collaboration(collaboration::Event::CreateRoom { name })
    }
    /// 构造协作加入房间事件
    pub fn collaboration_join_room(invite_code: String) -> Self {
        Self::Collaboration(collaboration::Event::JoinRoom { invite_code })
    }
    /// 构造协作断开事件
    pub const fn collaboration_disconnect() -> Self {
        Self::Collaboration(collaboration::Event::Disconnect)
    }
    /// 构造协作认证成功事件
    pub fn collaboration_authenticated(user_id: String, invite_code: String) -> Self {
        Self::Collaboration(collaboration::Event::Authenticated {
            user_id,
            invite_code,
        })
    }
    /// 构造协作房间已创建事件
    pub fn collaboration_room_created(
        room_name: String,
        invite_code: String,
        project_name: String,
        project_hash: String,
    ) -> Self {
        Self::Collaboration(collaboration::Event::RoomCreated {
            room_name,
            invite_code,
            project_name,
            project_hash,
        })
    }
    /// 构造协作房间已加入事件
    pub fn collaboration_room_joined(
        room_name: String,
        invite_code: String,
        user_count: usize,
        project_name: String,
        project_hash: String,
    ) -> Self {
        Self::Collaboration(collaboration::Event::RoomJoined {
            room_name,
            invite_code,
            user_count,
            project_name,
            project_hash,
        })
    }
    /// 构造协作已断开事件
    pub const fn collaboration_disconnected() -> Self {
        Self::Collaboration(collaboration::Event::Disconnected)
    }
    /// 构造协作连接失败事件
    pub fn collaboration_connect_failed(reason: String) -> Self {
        Self::Collaboration(collaboration::Event::ConnectFailed { reason })
    }
    /// 构造协作用户离开事件
    pub fn collaboration_user_left(user_id: String) -> Self {
        Self::Collaboration(collaboration::Event::UserLeft { user_id })
    }
    /// 构造协作远端鼠标移动事件
    pub fn collaboration_mouse_update(
        user_id: String,
        x: f32,
        y: f32,
        color: String,
        username: String,
    ) -> Self {
        Self::Collaboration(collaboration::Event::MouseUpdate {
            user_id,
            x,
            y,
            color,
            username,
        })
    }
    /// 构造协作音符更新事件
    pub fn collaboration_note_update(user_id: String, operation: String) -> Self {
        Self::Collaboration(collaboration::Event::NoteUpdate { user_id, operation })
    }
    /// 构造协作工程更新事件
    pub fn collaboration_project_update(user_id: String, update: String) -> Self {
        Self::Collaboration(collaboration::Event::ProjectUpdate { user_id, update })
    }
    /// 构造远端选择更新事件
    pub fn collaboration_selection(user_id: String, selection: String, color: String) -> Self {
        Self::Collaboration(collaboration::Event::Selection {
            user_id,
            selection,
            color,
        })
    }

    // ── 同步构造函数（直接构造 sync::Event） ──

    /// 构造本地音符添加同步事件
    ///
    /// 携带 `id`：发送端在发射时已从文档取回真实全局 ID（绘制直接取、
    /// 粘贴/复制/排布经 `note_id_at` 反查），由 runner 写入 `SyncNote.id`。
    pub fn local_note_added(
        id: u64,
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::LocalNoteAdded {
            id,
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        })
    }
    /// 构造本地音符移动同步事件
    pub fn local_note_moved(
        id: u64,
        tick: f32,
        key: u16,
        length: f32,
        tick_offset: f32,
        key_offset: i16,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::LocalNoteMoved {
            id,
            tick,
            key,
            length,
            tick_offset,
            key_offset,
            track_index,
        })
    }
    /// 构造本地音符删除同步事件
    pub fn local_note_deleted(
        id: u64,
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::LocalNoteDeleted {
            id,
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        })
    }
    /// 构造本地音轨添加同步事件
    pub fn local_track_added(track_index: usize) -> Self {
        Self::Sync(sync::Event::LocalTrackAdded { track_index })
    }
    /// 构造本地选择变更同步事件
    pub fn local_selection_changed(
        active: bool,
        timestamp: u64,
        fingerprints: Vec<[f64; 4]>,
    ) -> Self {
        Self::Sync(sync::Event::LocalSelectionChanged {
            active,
            timestamp,
            fingerprints,
        })
    }
}
