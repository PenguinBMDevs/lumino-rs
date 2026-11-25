//! 房间创建和加入

use tracing::info;

use crate::Result;
use crate::http::CreateRoomResponse;

use super::CollaborationClient;

impl CollaborationClient {
    /// 创建房间并连接
    pub async fn create_room_and_connect(
        &mut self,
        room_name: String,
    ) -> Result<CreateRoomResponse> {
        info!("通过 HTTP 创建房间: {}", room_name);
        let response = self
            .http_client
            .create_room(&room_name, &self.generate_user_id())
            .await?;

        info!(
            "房间创建成功: id={}, invite_code={}",
            response.room.id, response.room.invite_code
        );

        self.room_id = Some(response.room.invite_code.clone());

        info!("使用 roomId 连接 WebSocket");
        self.connect_with_room_id(&response.room.invite_code)
            .await?;
        Ok(response)
    }

    /// 加入房间并连接
    pub async fn join_room_and_connect(&mut self, invite_code: String) -> Result<()> {
        info!("准备加入房间: {}", invite_code);
        self.room_id = Some(invite_code.clone());

        tracing::debug!("准备调用 connect_with_room_id");
        self.connect_with_room_id(&invite_code).await?;
        tracing::debug!("connect_with_room_id 完成");
        Ok(())
    }
}
