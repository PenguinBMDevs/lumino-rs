//! Runner 协作：连接 / 房间生命周期与视图状态广播

use crate::runner::{CollaborationStatus, RunnerInner};
use lumino_ui::state::root_state::CollaborationViewState;

impl RunnerInner {
    /// 处理协作连接
    pub(crate) fn handle_collaboration_connect(
        &mut self,
        host: String,
        port: u16,
        username: String,
        password: String,
        room_name: Option<String>,
        invite_code: Option<String>,
    ) {
        // 更新状态为连接中，并广播到协作对话框（若存在）
        self.collab_state.collaboration_status = CollaborationStatus::Connecting;
        // 记录服务器地址（工程文件同步 HTTP 请求用）
        self.collab_state.server_host = host.clone();
        self.collab_state.server_port = port;
        self.set_main_collab_view_state(CollaborationViewState::Connecting, None, None);

        // 使用协作服务连接
        let service = self.collab_state.collaboration_service.clone();
        tokio::spawn(async move {
            if let Err(e) = service
                .connect(host, port, username, password, room_name, invite_code)
                .await
            {
                tracing::error!("协作连接失败: {}", e);
            }
        });
    }

    /// 将协作视图状态写入主窗口 Root，并广播到所有已打开的协作对话框。
    ///
    /// 主窗口 Root 是协作状态的唯一数据源；对话框为独立 Root，需通过
    /// `DialogManager::forward_collaboration_view_state` 同步最新视图状态，
    /// 否则对话框永远停在“连接中”而无法进入房间。
    /// 仅广播视图状态与房间信息，**排除连接表单字段**。返回是否发生了实际变更。
    pub(crate) fn set_main_collab_view_state(
        &mut self,
        state: CollaborationViewState,
        invite_code: Option<String>,
        room_name: Option<String>,
    ) -> bool {
        let changed = self
            .window_state
            .window
            .ui_mut()
            .set_collaboration_view_state(state, invite_code.clone(), room_name.clone());

        self.window_state
            .dialog_manager
            .forward_collaboration_view_state(state, invite_code, room_name);

        changed
    }

    /// 处理创建房间
    pub(crate) fn handle_collaboration_create_room(&self, name: String) {
        tracing::info!("协作: 请求创建房间 - {} (已转发到 UI 层)", name);
    }

    /// 处理加入房间
    pub(crate) fn handle_collaboration_join_room(&self, invite_code: String) {
        tracing::info!("协作: 请求加入房间 - {} (已转发到 UI 层)", invite_code);
    }

    /// 处理断开连接
    pub(crate) fn handle_collaboration_disconnect(&mut self) {
        tracing::info!("协作: 请求断开连接");
        if let Err(e) = self.collab_state.collaboration_service.disconnect() {
            tracing::error!("协作: 断开连接失败: {}", e);
        }
    }
}
