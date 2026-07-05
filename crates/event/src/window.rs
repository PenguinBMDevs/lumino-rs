pub mod collaboration;
pub mod dialog;
pub mod lifecycle;
pub mod sync;

#[derive(Debug, Clone)]
/// 窗口事件
pub enum Event {
    Lifecycle(lifecycle::Event),
    Dialog(dialog::Event),
    Collaboration(collaboration::Event),
    Sync(sync::Event),
}

impl Event {
    /// 获取事件的人类可读显示名称
    pub fn display_name(&self) -> String {
        match self {
            Self::Lifecycle(e) => e.display_name(),
            Self::Dialog(e) => e.display_name(),
            Self::Collaboration(e) => e.display_name(),
            Self::Sync(e) => e.display_name(),
        }
    }

    // ── 构造函数（替代 event! 宏，IDE 友好） ──

    pub const fn drag() -> Self {
        Self::Lifecycle(lifecycle::Event::drag())
    }
    pub const fn close() -> Self {
        Self::Lifecycle(lifecycle::Event::close())
    }
    pub const fn toggle_maximize() -> Self {
        Self::Lifecycle(lifecycle::Event::toggle_maximize())
    }
    pub const fn maximize() -> Self {
        Self::Lifecycle(lifecycle::Event::maximize())
    }
    pub const fn minimize() -> Self {
        Self::Lifecycle(lifecycle::Event::minimize())
    }

    pub const fn open_custom_precision_dialog() -> Self {
        Self::Dialog(dialog::Event::open_custom_precision_dialog())
    }
    pub const fn close_custom_precision_dialog() -> Self {
        Self::Dialog(dialog::Event::close_custom_precision_dialog())
    }
    pub const fn apply_custom_precision(numerator: u32, denominator: u32) -> Self {
        Self::Dialog(dialog::Event::apply_custom_precision(
            numerator,
            denominator,
        ))
    }
    pub fn open_load_confirm_dialog(path: String, size_mb: f64) -> Self {
        Self::Dialog(dialog::Event::open_load_confirm_dialog(path, size_mb))
    }
    pub const fn open_collaboration_dialog() -> Self {
        Self::Dialog(dialog::Event::open_collaboration_dialog())
    }
    pub const fn close_collaboration_dialog() -> Self {
        Self::Dialog(dialog::Event::close_collaboration_dialog())
    }
    pub const fn open_speed_change_dialog() -> Self {
        Self::Dialog(dialog::Event::open_speed_change_dialog())
    }
    pub const fn close_speed_change_dialog() -> Self {
        Self::Dialog(dialog::Event::close_speed_change_dialog())
    }
    pub const fn confirm_speed_change(factor: f32) -> Self {
        Self::Dialog(dialog::Event::confirm_speed_change(factor))
    }
    pub const fn open_project_settings_dialog() -> Self {
        Self::Dialog(dialog::Event::open_project_settings_dialog())
    }
    pub const fn close_project_settings_dialog() -> Self {
        Self::Dialog(dialog::Event::close_project_settings_dialog())
    }
    pub fn apply_project_settings(title: String, tempo: f64, copyright: String) -> Self {
        Self::Dialog(dialog::Event::apply_project_settings(
            title, tempo, copyright,
        ))
    }

    pub fn collaboration_connect(
        host: String,
        port: u16,
        username: String,
        invite_code: Option<String>,
    ) -> Self {
        Self::Collaboration(collaboration::Event::connect(
            host,
            port,
            username,
            invite_code,
        ))
    }
    pub fn collaboration_create_room(name: String) -> Self {
        Self::Collaboration(collaboration::Event::create_room(name))
    }
    pub fn collaboration_join_room(invite_code: String) -> Self {
        Self::Collaboration(collaboration::Event::join_room(invite_code))
    }
    pub const fn collaboration_disconnect() -> Self {
        Self::Collaboration(collaboration::Event::disconnect())
    }
    pub fn collaboration_authenticated(user_id: String, invite_code: String) -> Self {
        Self::Collaboration(collaboration::Event::authenticated(user_id, invite_code))
    }
    pub fn collaboration_room_created(room_name: String, invite_code: String) -> Self {
        Self::Collaboration(collaboration::Event::room_created(room_name, invite_code))
    }
    pub fn collaboration_room_joined(
        room_name: String,
        invite_code: String,
        user_count: usize,
    ) -> Self {
        Self::Collaboration(collaboration::Event::room_joined(
            room_name,
            invite_code,
            user_count,
        ))
    }
    pub const fn collaboration_disconnected() -> Self {
        Self::Collaboration(collaboration::Event::disconnected())
    }
    pub fn collaboration_user_left(user_id: String) -> Self {
        Self::Collaboration(collaboration::Event::user_left(user_id))
    }
    pub fn collaboration_mouse_update(
        user_id: String,
        x: f32,
        y: f32,
        color: String,
        username: String,
    ) -> Self {
        Self::Collaboration(collaboration::Event::mouse_update(
            user_id, x, y, color, username,
        ))
    }
    pub fn collaboration_note_update(user_id: String, operation: String) -> Self {
        Self::Collaboration(collaboration::Event::note_update(user_id, operation))
    }
    pub fn collaboration_project_update(user_id: String, update: String) -> Self {
        Self::Collaboration(collaboration::Event::project_update(user_id, update))
    }

    pub fn local_note_added(
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::local_note_added(
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        ))
    }
    pub fn local_note_moved(
        tick: f32,
        key: u16,
        length: f32,
        tick_offset: f32,
        key_offset: i16,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::local_note_moved(
            tick,
            key,
            length,
            tick_offset,
            key_offset,
            track_index,
        ))
    }
    pub fn local_note_deleted(
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    ) -> Self {
        Self::Sync(sync::Event::local_note_deleted(
            tick,
            key,
            length,
            velocity,
            channel,
            track_index,
        ))
    }
    pub fn local_track_added(track_index: usize) -> Self {
        Self::Sync(sync::Event::local_track_added(track_index))
    }
}
