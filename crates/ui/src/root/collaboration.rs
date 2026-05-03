//! Root 协作功能处理器子模块

use crate::root::Root;
use crate::state::root_state::{CollaborationViewState, DialogType};

impl Root {
    /// 打开协作对话框并设置为连接中状态（用于调试模式自动连接）
    pub fn open_collaboration_dialog_with_state(
        &mut self,
        host: String,
        port: u16,
        username: String,
    ) {
        // 设置对话框字段
        self.state.collaboration_dialog.server_host = host.clone();
        self.state.collaboration_dialog.server_port = port.to_string();
        self.state.collaboration_dialog.username = username;
        self.state.collaboration_dialog.is_open = true;
        self.state.dialog_type = DialogType::Collaboration;

        // 设置为正在连接状态
        self.state.collaboration_dialog.view_state = CollaborationViewState::Connecting;
        self.state.collaboration_dialog.connection_status = "正在连接...".to_string();

        tracing::info!("协作对话框已打开，正在连接服务器 {}:{}", host, port);
    }

    /// 从另一个 Root 同步协作状态（用于对话框窗口同步主窗口状态）
    pub fn sync_collaboration_state_from(&mut self, other: &Root) {
        // 复制协作对话框状态
        self.state.collaboration_dialog = other.state.collaboration_dialog.clone();
        self.state.collaboration_dialog.is_open = true;
        self.state.dialog_type = DialogType::Collaboration;

        tracing::info!(
            "协作对话框状态已同步: view_state={:?}, invite_code={}",
            self.state.collaboration_dialog.view_state,
            self.state.collaboration_dialog.invite_code
        );
    }

    /// 处理加载确认对话框 - 确认
    pub(super) fn handle_confirm_load(&mut self) {
        // 设置对话框结果（供独立窗口模式使用）
        self.state.dialog_result = Some(crate::host::DialogResult::LoadConfirm);

        self.state.load_confirm_dialog.is_open = false;
        self.state.dialog_type = crate::state::root_state::DialogType::None;
    }

    /// 处理加载确认对话框 - 取消
    pub(super) fn handle_cancel_load(&mut self) {
        self.state.load_confirm_dialog.is_open = false;
        self.state.dialog_type = crate::state::root_state::DialogType::None;
    }
}
