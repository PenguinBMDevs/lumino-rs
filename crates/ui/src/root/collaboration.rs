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
}
