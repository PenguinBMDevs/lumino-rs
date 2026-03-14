//! Runner 协作处理

use crate::runner::{CollaborationStatus, RunnerInner};

impl RunnerInner {
    /// 同步协作状态（发送鼠标位置等）
    pub(super) fn sync_collaboration_state(&mut self) {
        if let Some(client) = &self.collaboration_client {
            let ui = self.window.ui();
            let editor = ui.root().editor_ref();

            if let Some(pos) = editor.cursor_position {
                // 转换为 Canvas 相对坐标并考虑滚动
                let local_pos = iced_core::Point::new(
                    pos.x - editor.canvas_offset.x,
                    pos.y - editor.canvas_offset.y,
                );

                if editor.is_inside_canvas(local_pos) {
                    let client = client.clone();
                    let scroll_x = editor.state.scroll_x;
                    let scroll_y = editor.state.scroll_y;
                    let zoom_x = editor.state.zoom_x;
                    let zoom_y = editor.state.zoom_y;

                    let pos = lumino_collaboration::types::MousePosition {
                        x: local_pos.x,
                        y: local_pos.y,
                        view_state: Some(lumino_collaboration::types::ViewState {
                            scroll_x,
                            scroll_y,
                            zoom_x,
                            zoom_y,
                            ..Default::default()
                        }),
                    };

                    tokio::spawn(async move {
                        let c = client.lock().await;
                        let _ = c.send_mouse_position(pos);
                    });
                }
            }
        }
    }

    /// 处理协作连接
    pub(super) fn handle_collaboration_connect(
        &mut self,
        host: String,
        port: u16,
        username: String,
    ) {
        // 更新状态为连接中
        self.collaboration_status = CollaborationStatus::Connecting;

        // 使用协作服务连接
        let service = self.collaboration_service.clone();
        tokio::spawn(async move {
            if let Err(e) = service.connect(host, port, username).await {
                tracing::error!("协作连接失败: {}", e);
            }
        });
    }

    /// 处理创建房间
    pub(super) fn handle_collaboration_create_room(&self, name: String) {
        tracing::info!("协作: 请求创建房间 - {}", name);
        let handler = self.collaboration_handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handler.create_room(name) {
                tracing::error!("协作: 创建房间失败: {}", e);
            }
        });
    }

    /// 处理加入房间
    pub(super) fn handle_collaboration_join_room(&self, invite_code: String) {
        tracing::info!("协作: 请求加入房间 - {}", invite_code);
        let handler = self.collaboration_handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handler.join_room(invite_code) {
                tracing::error!("协作: 加入房间失败: {}", e);
            }
        });
    }

    /// 处理断开连接
    pub(super) fn handle_collaboration_disconnect(&mut self) {
        tracing::info!("协作: 请求断开连接");
        let mut handler = self.collaboration_handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handler.disconnect().await {
                tracing::error!("协作: 断开连接失败: {}", e);
            }
        });
    }
}
