//! Host 对话框和协作子模块 - 处理对话框状态和远程协作

use crate::host::{Host, types::DialogResult};
use crate::state::root_state::CollaborationViewState;
use crate::{message, window};

impl Host {
    /// 设置加载确认对话框（用于独立对话框窗口）
    pub fn set_load_confirm_dialog(&mut self, file_path: &str, size_mb: f64) {
        self.root.set_load_confirm_dialog(file_path, size_mb);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置自定义精度对话框是否打开（用于独立对话框窗口）
    pub fn set_custom_precision_dialog_open(&mut self, open: bool) {
        self.root.set_custom_precision_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置工程设置对话框是否打开（用于独立对话框窗口）
    pub fn set_project_settings_dialog_open(&mut self, open: bool) {
        self.root.set_project_settings_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 应用工程设置到主窗口
    pub fn apply_project_settings(&mut self, title: String, tempo: f64, copyright: String) {
        self.root.apply_project_settings(title, tempo, copyright);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 获取并清空对话框结果
    pub fn take_dialog_result(&mut self) -> Option<DialogResult> {
        self.root.take_dialog_result()
    }

    /// 设置自定义精度值（用于独立对话框窗口）
    pub fn set_custom_precision(&mut self, ticks: f32) {
        self.root.set_custom_precision(ticks);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置协作对话框是否打开（用于独立对话框窗口）
    pub fn set_collaboration_dialog_open(&mut self, open: bool) {
        self.root.set_collaboration_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置协作视图状态（用于独立对话框窗口）
    pub fn set_collaboration_view_state(
        &mut self,
        state: CollaborationViewState,
        invite_code: Option<String>,
        room_name: Option<String>,
    ) {
        self.root
            .set_collaboration_view_state(state, invite_code, room_name);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: String,
        x: f32,
        y: f32,
        color: String,
        username: String,
    ) {
        self.root
            .update(message::Message::CollaborationRemoteMouseMoved {
                user_id: user_id.into(),
                x,
                y,
                color: color.into(),
                username: username.into(),
            });
        self.window_ctx.window.request_redraw();
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: String) {
        self.root
            .update(message::Message::CollaborationRemoteUserLeft {
                user_id: user_id.into(),
            });
        self.window_ctx.window.request_redraw();
    }

    /// 更新远端音符
    pub fn update_remote_note(&mut self, operation: String) {
        self.root
            .update(message::Message::CollaborationRemoteNoteUpdate { operation });
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

    /// 获取当前 PPQ (Pulses Per Quarter note)
    pub fn ppq(&self) -> u16 {
        self.root.editor.editor_state.view.ppq
    }

    /// 更新进度
    pub fn update_progress(&mut self, progress: Option<(String, f64)>) {
        self.root.update(message::Message::Progress(progress));
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 更新主题
    pub fn update_theme(&mut self, theme: String) {
        self.root.update(window::Event::theme(theme));
        self.root.editor.grid_cache.clear();
        self.root.editor.keyboard_cache.clear();
        self.root.editor.ruler_cache.clear();
        self.render_ctx.render_cache.grid_viewport_hash = 0;
        self.render_ctx.render_cache.note_viewport_hash = 0;
        self.ui_dirty = true;
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
