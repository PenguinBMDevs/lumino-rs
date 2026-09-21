//! 协作视图状态、远程光标与选择同步

use crate::root::Root;
use crate::state::root_state::{CollaborationViewState, DialogType};

impl Root {
    /// 设置协作对话框是否打开
    pub fn set_collaboration_dialog_open(&mut self, open: bool) {
        self.state.collaboration_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::Collaboration;
            self.state.collaboration_dialog.view_state = CollaborationViewState::Connect;
            self.state.collaboration_dialog.connection_status = "未连接".to_string();
        }
        tracing::info!("协作对话框状态: {}", open);
    }

    /// 设置协作视图状态
    ///
    /// 返回视图状态是否发生变更（供 runner 决定是否广播），避免无谓的对话框刷新。
    pub fn set_collaboration_view_state(
        &mut self,
        state: CollaborationViewState,
        invite_code: Option<String>,
        room_name: Option<String>,
    ) -> bool {
        let changed = self.state.collaboration_dialog.view_state != state;
        self.state.collaboration_dialog.view_state = state;
        if let Some(code) = invite_code {
            self.state.collaboration_dialog.invite_code = code;
        }
        if let Some(name) = &room_name {
            self.state.collaboration_dialog.room_name = name.clone();
        }
        match state {
            CollaborationViewState::Connect => {
                // 连接失败时通过 room_name 参数透传具体原因（如"用户不存在"/"密码错误"）；
                // 否则保持默认"未连接"提示。
                self.state.collaboration_dialog.connection_status =
                    room_name.unwrap_or_else(|| "未连接".to_string());
            }
            CollaborationViewState::Connecting => {
                self.state.collaboration_dialog.connection_status = "正在连接...".to_string();
            }
            CollaborationViewState::RoomActions => {
                self.state.collaboration_dialog.connection_status =
                    "已连接，请创建或加入房间".to_string();
            }
            CollaborationViewState::InRoom => {
                self.state.collaboration_dialog.connection_status = format!(
                    "房间: {} | 邀请码: {}",
                    self.state.collaboration_dialog.room_name,
                    self.state.collaboration_dialog.invite_code
                );
            }
        }
        tracing::info!("协作视图状态已更新: {:?}", state);
        changed
    }

    /// 更新远程光标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: u64,
        position: iced_core::Point,
        color: [f32; 4],
        username: String,
    ) {
        // 将颜色数组转换为十六进制字符串
        let color_str = format!(
            "{:02X}{:02X}{:02X}",
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8
        );
        self.editor.update_remote_cursor(
            user_id.to_string().into(),
            position.x,
            position.y,
            color_str.into(),
            username.into(),
        );
    }

    /// 更新远程音符
    pub fn update_remote_note(&mut self, user_id: u64, operation: String) {
        // 这里将来可以解析 JSON 并应用到编辑器
        tracing::info!(
            "协作: 处理远端音符更新 - 用户: {}, 操作: {}",
            user_id,
            operation
        );
    }

    /// 应用远端用户的选择更新到本地编辑器（高亮 + 冲突判定）
    pub fn apply_remote_selection(&mut self, user_id: String, selection: String, color: String) {
        self.editor
            .apply_remote_selection(&user_id, &selection, &color);
    }
}
