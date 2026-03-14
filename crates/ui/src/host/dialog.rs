//! Host 对话框和协作子模块 - 处理对话框状态和远程协作

use crate::host::{Host, types::DialogResult};
use crate::state::root_state::CollaborationViewState;
use crate::{message, window};

impl Host {
    /// 设置自定义精度对话框是否打开（用于独立对话框窗口）
    pub fn set_custom_precision_dialog_open(&mut self, open: bool) {
        self.root.set_custom_precision_dialog_open(open);
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 获取并清空对话框结果
    pub fn take_dialog_result(&mut self) -> Option<DialogResult> {
        self.root.take_dialog_result()
    }

    /// 设置自定义精度值（用于独立对话框窗口）
    pub fn set_custom_precision(&mut self, ticks: f32) {
        self.root.set_custom_precision(ticks);
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 设置协作对话框是否打开（用于独立对话框窗口）
    pub fn set_collaboration_dialog_open(&mut self, open: bool) {
        self.root.set_collaboration_dialog_open(open);
        self.clear_cache();
        self.window.request_redraw();
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
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 更新远端鼠标位置
    pub fn update_remote_cursor(&mut self, user_id: String, x: f32, y: f32, color: String) {
        self.root
            .update(message::Message::CollaborationRemoteMouseMoved {
                user_id,
                x,
                y,
                color,
            });
        self.window.request_redraw();
    }

    /// 更新远端音符
    pub fn update_remote_note(&mut self, user_id: String, operation: String) {
        self.root
            .update(message::Message::CollaborationRemoteNoteUpdate { user_id, operation });
        self.window.request_redraw();
    }

    /// 获取当前 PPQ (Pulses Per Quarter note)
    pub fn ppq(&self) -> u16 {
        self.root.editor.state.ppq
    }

    /// 更新进度
    pub fn update_progress(&mut self, progress: Option<(String, f64)>) {
        self.root.update(message::Message::Progress(progress));
    }

    /// 更新主题
    pub fn update_theme(&mut self, theme: String) {
        self.root.update(window::Event::theme(theme));
        self.clear_cache();
        self.window.request_redraw();
    }
}
