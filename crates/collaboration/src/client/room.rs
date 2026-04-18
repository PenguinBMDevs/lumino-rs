//! 房间创建和加入

use tracing::info;

use crate::Result;
use crate::http::CreateRoomResponse;
use crate::types::RoomInfo;

use super::CollaborationClient;

impl CollaborationClient {
    /// 创建房间（遗留兼容接口）
    pub async fn create_room(&self, _name: String) -> Result<()> {
        Err("请使用 create_room_and_connect".into())
    }

    /// 加入房间（遗留兼容接口）
    pub async fn join_room(&self, _invite_code: String) -> Result<()> {
        Err("请使用 join_room_and_connect".into())
    }

    /// 创建房间并连接
    pub async fn create_room_and_connect(
        &mut self,
        room_name: String,
    ) -> Result<CreateRoomResponse> {
        let create_response = self.create_room_via_http(&room_name).await?;
        self.save_room_info_and_connect(&create_response).await?;
        Ok(create_response)
    }

    /// 通过 HTTP 创建房间
    async fn create_room_via_http(&self, room_name: &str) -> Result<CreateRoomResponse> {
        info!("通过 HTTP 创建房间: {}", room_name);
        let response = self
            .http_client
            .create_room(room_name, &self.generate_user_id())
            .await?;

        info!(
            "房间创建成功: id={}, invite_code={}",
            response.room.id, response.room.invite_code
        );

        Ok(response)
    }

    /// 保存房间信息并连接
    async fn save_room_info_and_connect(
        &mut self,
        create_response: &CreateRoomResponse,
    ) -> Result<()> {
        self.room_id = Some(create_response.room.invite_code.clone());
        self.update_session_with_room_info(create_response).await;

        info!("使用 roomId 连接 WebSocket");
        self.connect_with_room_id(&create_response.room.invite_code)
            .await
    }

    /// 更新会话信息
    async fn update_session_with_room_info(&self, response: &CreateRoomResponse) {
        let mut session = self.session.write().await;
        session.invite_code = Some(response.room.invite_code.clone());
        session.current_room = Some(RoomInfo {
            id: response.room.id.clone(),
            invite_code: response.room.invite_code.clone(),
            name: response.room.name.clone(),
            host_id: response.room.host_id.clone(),
            user_count: 1,
            max_users: 10,
        });
    }

    /// 加入房间并连接
    pub async fn join_room_and_connect(&mut self, invite_code: String) -> Result<()> {
        info!("准备加入房间: {}", invite_code);
        self.save_invite_code(&invite_code).await;
        self.connect_to_room(&invite_code).await
    }

    /// 保存邀请码到会话
    async fn save_invite_code(&mut self, invite_code: &str) {
        self.room_id = Some(invite_code.to_string());
        let mut session = self.session.write().await;
        session.invite_code = Some(invite_code.to_string());
    }

    /// 连接到房间
    async fn connect_to_room(&mut self, invite_code: &str) -> Result<()> {
        tracing::debug!("准备调用 connect_with_room_id");
        self.connect_with_room_id(invite_code).await?;
        tracing::debug!("connect_with_room_id 完成");
        Ok(())
    }
}
