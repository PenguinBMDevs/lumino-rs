pub mod audio;
pub mod collaboration;
pub mod dialog;
pub mod lifecycle;
pub mod sync;
pub mod video;

use std::sync::Arc;

#[derive(Debug, Clone)]
/// 窗口事件
pub enum Event {
    Lifecycle(lifecycle::Event),
    Dialog(dialog::Event),
    Collaboration(collaboration::Event),
    Sync(sync::Event),
}

impl Event {
    // ── 生命周期构造函数（直接构造，无需中间函数） ──

    pub const fn drag() -> Self {
        Self::Lifecycle(lifecycle::Event::Drag)
    }
    pub const fn close() -> Self {
        Self::Lifecycle(lifecycle::Event::Close)
    }
    pub const fn toggle_maximize() -> Self {
        Self::Lifecycle(lifecycle::Event::ToggleMaximize)
    }
    pub const fn maximize() -> Self {
        Self::Lifecycle(lifecycle::Event::Maximize)
    }
    pub const fn minimize() -> Self {
        Self::Lifecycle(lifecycle::Event::Minimize)
    }

    // ── 对话框构造函数（直接构造 dialog::Event） ──

    pub const fn open_custom_precision_dialog() -> Self {
        Self::Dialog(dialog::Event::OpenCustomPrecisionDialog)
    }
    pub const fn close_custom_precision_dialog() -> Self {
        Self::Dialog(dialog::Event::CloseCustomPrecisionDialog)
    }
    pub const fn apply_custom_precision(numerator: u32, denominator: u32) -> Self {
        Self::Dialog(dialog::Event::ApplyCustomPrecision(numerator, denominator))
    }
    pub fn open_load_confirm_dialog(path: String, size_mb: f64) -> Self {
        Self::Dialog(dialog::Event::OpenLoadConfirmDialog { path, size_mb })
    }
    pub const fn open_collaboration_dialog() -> Self {
        Self::Dialog(dialog::Event::OpenCollaborationDialog)
    }
    pub const fn close_collaboration_dialog() -> Self {
        Self::Dialog(dialog::Event::CloseCollaborationDialog)
    }
    pub const fn open_speed_change_dialog() -> Self {
        Self::Dialog(dialog::Event::OpenSpeedChangeDialog)
    }
    pub const fn close_speed_change_dialog() -> Self {
        Self::Dialog(dialog::Event::CloseSpeedChangeDialog)
    }
    pub const fn confirm_speed_change(factor: f32) -> Self {
        Self::Dialog(dialog::Event::ConfirmSpeedChange(factor))
    }
    pub const fn open_batch_edit_dialog() -> Self {
        Self::Dialog(dialog::Event::OpenBatchEditDialog)
    }
    pub const fn close_batch_edit_dialog() -> Self {
        Self::Dialog(dialog::Event::CloseBatchEditDialog)
    }
    pub fn confirm_batch_edit(velocity: String, gate: String, key: String, tick: String) -> Self {
        Self::Dialog(dialog::Event::ConfirmBatchEdit {
            velocity,
            gate,
            key,
            tick,
        })
    }
    pub const fn open_video_export_dialog() -> Self {
        Self::Dialog(dialog::Event::OpenVideoExportDialog)
    }
    pub const fn close_video_export_dialog() -> Self {
        Self::Dialog(dialog::Event::CloseVideoExportDialog)
    }
    pub const fn open_project_settings_dialog() -> Self {
        Self::Dialog(dialog::Event::OpenProjectSettingsDialog)
    }
    pub const fn close_project_settings_dialog() -> Self {
        Self::Dialog(dialog::Event::CloseProjectSettingsDialog)
    }
    pub const fn open_memory_monitor_dialog() -> Self {
        Self::Dialog(dialog::Event::OpenMemoryMonitorDialog)
    }
    pub const fn close_memory_monitor_dialog() -> Self {
        Self::Dialog(dialog::Event::CloseMemoryMonitorDialog)
    }
    pub fn apply_project_settings(
        title: String,
        tempo: f64,
        copyright: String,
        time_signatures: Vec<(u32, u8, u8)>,
    ) -> Self {
        Self::Dialog(dialog::Event::ApplyProjectSettings {
            title,
            tempo,
            copyright,
            time_signatures,
        })
    }
    pub fn start_audio_export(
        config: dialog::AudioExportConfig,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) -> Self {
        Self::Dialog(dialog::Event::StartAudioExport { config, document })
    }
    pub fn start_video_export(
        config: dialog::VideoExportConfig,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) -> Self {
        Self::Dialog(dialog::Event::StartVideoExport { config, document })
    }

    // ── 协作构造函数（直接构造 collaboration::Event） ──

    pub fn collaboration_connect(
        host: String,
        port: u16,
        username: String,
        invite_code: Option<String>,
    ) -> Self {
        Self::Collaboration(collaboration::Event::Connect {
            host,
            port,
            username,
            invite_code,
        })
    }
    pub fn collaboration_create_room(name: String) -> Self {
        Self::Collaboration(collaboration::Event::CreateRoom { name })
    }
    pub fn collaboration_join_room(invite_code: String) -> Self {
        Self::Collaboration(collaboration::Event::JoinRoom { invite_code })
    }
    pub const fn collaboration_disconnect() -> Self {
        Self::Collaboration(collaboration::Event::Disconnect)
    }
    pub fn collaboration_authenticated(user_id: String, invite_code: String) -> Self {
        Self::Collaboration(collaboration::Event::Authenticated {
            user_id,
            invite_code,
        })
    }
    pub fn collaboration_room_created(room_name: String, invite_code: String) -> Self {
        Self::Collaboration(collaboration::Event::RoomCreated {
            room_name,
            invite_code,
        })
    }
    pub fn collaboration_room_joined(
        room_name: String,
        invite_code: String,
        user_count: usize,
    ) -> Self {
        Self::Collaboration(collaboration::Event::RoomJoined {
            room_name,
            invite_code,
            user_count,
        })
    }
    pub const fn collaboration_disconnected() -> Self {
        Self::Collaboration(collaboration::Event::Disconnected)
    }
    pub fn collaboration_user_left(user_id: String) -> Self {
        Self::Collaboration(collaboration::Event::UserLeft { user_id })
    }
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
    pub fn collaboration_note_update(user_id: String, operation: String) -> Self {
        Self::Collaboration(collaboration::Event::NoteUpdate { user_id, operation })
    }
    pub fn collaboration_project_update(user_id: String, update: String) -> Self {
        Self::Collaboration(collaboration::Event::ProjectUpdate { user_id, update })
    }

    // ── 同步构造函数（直接构造 sync::Event） ──

    pub fn local_note_added(
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::LocalNoteAdded {
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        })
    }
    pub fn local_note_moved(
        tick: f32,
        key: u16,
        length: f32,
        tick_offset: f32,
        key_offset: i16,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::LocalNoteMoved {
            tick,
            key,
            length,
            tick_offset,
            key_offset,
            track_index,
        })
    }
    pub fn local_note_deleted(
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::LocalNoteDeleted {
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        })
    }
    pub fn local_track_added(track_index: usize) -> Self {
        Self::Sync(sync::Event::LocalTrackAdded { track_index })
    }
}
