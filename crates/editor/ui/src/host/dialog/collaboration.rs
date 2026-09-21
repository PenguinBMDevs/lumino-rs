//! Host 协作子模块 — 远端光标/选择/音符操作与协作状态同步

use crate::host::Host;
use crate::message;

impl Host {
    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: String,
        x: f32,
        y: f32,
        color: String,
        username: String,
    ) {
        self.route_message(message::Message::Collaboration(
            lumino_message::CollaborationAction::RemoteMouseMoved {
                user_id: user_id.into(),
                x,
                y,
                color: color.into(),
                username: username.into(),
            },
        ));
        self.window_ctx.window.request_redraw();
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: String) {
        self.route_message(message::Message::Collaboration(
            lumino_message::CollaborationAction::RemoteUserLeft {
                user_id: user_id.into(),
            },
        ));
        self.window_ctx.window.request_redraw();
    }

    /// 应用远端用户的选择更新（高亮 + first-writer-wins 冲突判定）
    pub fn apply_remote_selection(&mut self, user_id: String, selection: String, color: String) {
        self.route_message(message::Message::Collaboration(
            lumino_message::CollaborationAction::RemoteSelection {
                user_id: user_id.into(),
                selection,
                color: color.into(),
            },
        ));
        self.window_ctx.window.request_redraw();
    }

    /// 更新远端音符
    pub fn update_remote_note(&mut self, operation: String) {
        self.route_message(message::Message::Collaboration(
            lumino_message::CollaborationAction::RemoteNoteUpdate { operation },
        ));
        self.window_ctx.window.request_redraw();
    }

    /// 应用远程笔记操作到本地编辑器（委托给 Root 实现）
    pub fn apply_remote_note_operation(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        self.root.apply_remote_note_operation(operation);
        self.window_ctx.window.request_redraw();
    }

    /// 添加远程音轨（委托给 Root 实现）
    pub fn add_remote_track(&mut self, track_idx: usize) {
        self.root.add_remote_track(track_idx);
        self.window_ctx.window.request_redraw();
    }

    /// 打开协作对话框并设置为连接中状态（用于调试模式自动连接）
    pub fn open_collaboration_dialog_with_state(
        &mut self,
        host: String,
        port: u16,
        username: String,
    ) {
        self.root
            .open_collaboration_dialog_with_state(host, port, username);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 从另一个 Host 同步协作状态（用于对话框窗口同步主窗口状态）
    pub fn sync_collaboration_state_from(&mut self, other: &Host) {
        self.root.sync_collaboration_state_from(&other.root);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }
}
