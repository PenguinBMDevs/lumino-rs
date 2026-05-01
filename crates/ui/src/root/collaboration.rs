//! Root 协作功能处理器子模块

use crate::root::Root;
use crate::state::root_state::{CollaborationViewState, DialogType};
use crate::toolbar;

impl Root {
    /// 处理打开协作对话框
    pub(super) fn handle_collaboration_dialog_open(&mut self) {
        lumino_core::event::emit(lumino_core::event::Event::Window(
            lumino_core::event::window::Event::OpenCollaborationDialog,
        ));
    }

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

    /// 处理关闭协作对话框
    pub(super) fn handle_collaboration_dialog_close(&mut self) {
        if self.state.is_dialog_window {
            lumino_core::event::emit(lumino_core::event::Event::Window(
                lumino_core::event::window::Event::CloseCollaborationDialog,
            ));
        }
    }

    /// 处理协作连接
    pub(super) fn handle_collaboration_connect(
        &mut self,
        host: String,
        port: u16,
        username: String,
        invite_code: Option<String>,
    ) {
        tracing::info!("协作: 连接服务器 {}:{}", host, port);
        // 设置连接状态为正在连接
        self.state.collaboration_dialog.connection_status = "正在连接...".to_string();
        lumino_core::event::emit(lumino_core::event::Event::Window(
            lumino_core::event::window::Event::CollaborationConnect {
                host,
                port,
                username,
                invite_code,
            },
        ));
    }

    /// 处理创建房间
    pub(super) fn handle_collaboration_create_room(&mut self, name: String) {
        tracing::info!("协作: 创建房间 {}", name);
        lumino_core::event::emit(lumino_core::event::Event::Window(
            lumino_core::event::window::Event::CollaborationCreateRoom { name },
        ));
    }

    /// 处理加入房间
    pub(super) fn handle_collaboration_join_room(&mut self, invite_code: String) {
        tracing::info!("协作: 加入房间 {}", invite_code);
        lumino_core::event::emit(lumino_core::event::Event::Window(
            lumino_core::event::window::Event::CollaborationJoinRoom { invite_code },
        ));
    }

    /// 处理断开连接
    pub(super) fn handle_collaboration_disconnect(&mut self) {
        tracing::info!("协作: 断开连接");
        lumino_core::event::emit(lumino_core::event::Event::Window(
            lumino_core::event::window::Event::CollaborationDisconnect,
        ));
        self.state.collaboration_dialog.reset();
    }

    /// 处理复制邀请码
    pub(super) fn handle_collaboration_copy_invite_code(&mut self) {
        let invite_code = self.state.collaboration_dialog.invite_code.clone();
        if invite_code.is_empty() {
            return;
        }

        // 复制到剪贴板
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                if let Err(e) = clipboard.set_text(&invite_code) {
                    tracing::error!("复制邀请码失败: {}", e);
                } else {
                    tracing::info!("邀请码已复制: {}", invite_code);
                }
            }
            Err(e) => {
                tracing::error!("创建剪贴板失败: {}", e);
            }
        }
    }

    /// 处理打开自定义精度对话框
    pub(super) fn handle_custom_precision_dialog_open(&mut self) {
        lumino_core::event::emit(lumino_core::event::Event::Window(
            lumino_core::event::window::Event::OpenCustomPrecisionDialog,
        ));
    }

    /// 处理关闭自定义精度对话框
    pub(super) fn handle_custom_precision_dialog_close(&mut self) {
        if self.state.is_dialog_window {
            lumino_core::event::emit(lumino_core::event::Event::Window(
                lumino_core::event::window::Event::CloseCustomPrecisionDialog,
            ));
        }
        self.state.custom_precision_dialog.is_open = false;
    }

    /// 处理确认自定义精度
    pub(super) fn handle_confirm_custom_precision(&mut self) {
        // 确认自定义精度，计算并设置结果
        let tuplet_count = self.state.custom_precision_dialog.tuplet_count.clone();
        let note_value = self.state.custom_precision_dialog.note_value.clone();

        // 设置对话框结果（供独立窗口模式使用）
        self.state.dialog_result = Some(crate::host::DialogResult::CustomPrecision {
            numerator: tuplet_count,
            denominator: note_value,
        });

        // 同时在主窗口应用（兼容模式）
        if let Some(ticks) = self
            .state
            .custom_precision_dialog
            .calculate_ticks(self.editor.state.ppq as u32)
        {
            self.state.note_precision = toolbar::NotePrecision::Custom;
            self.editor.state.snap_precision = ticks;
            self.editor.state.default_note_length = ticks;
            tracing::debug!("Root: 自定义精度应用为 {} ticks", ticks);
        }
        self.state.custom_precision_dialog.is_open = false;
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
