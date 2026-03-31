//! HTTP API 客户端
//!
//! 用于与服务器 HTTP API 交互（创建房间、获取房间信息等）

use reqwest::Client;
use serde::{Deserialize, Serialize};

/// HTTP API 客户端
pub struct HttpClient {
    client: Client,
    base_url: String,
}

/// 创建房间请求
#[derive(Debug, Serialize)]
pub struct CreateRoomRequest {
    pub name: String,
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "hostName", skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,
}

/// 创建房间响应
#[derive(Debug, Deserialize)]
pub struct CreateRoomResponse {
    pub success: bool,
    pub room: RoomInfo,
    #[serde(rename = "webSocketUrl")]
    pub web_socket_url: String,
}

/// 房间信息
#[derive(Debug, Deserialize, Clone)]
pub struct RoomInfo {
    pub id: String,
    #[serde(rename = "inviteCode")]
    pub invite_code: String,
    pub name: String,
    #[serde(rename = "hostId")]
    pub host_id: String,
}

impl HttpClient {
    /// 创建新的 HTTP 客户端
    pub fn new(host: &str, port: u16) -> Self {
        let protocol = if port == 443 { "https" } else { "http" };
        let base_url = if port == 80 || port == 443 {
            format!("{}://{}", protocol, host)
        } else {
            format!("{}://{}:{}", protocol, host, port)
        };

        Self {
            client: Client::new(),
            base_url,
        }
    }

    /// 创建房间
    pub async fn create_room(
        &self,
        name: &str,
        host_id: &str,
    ) -> Result<CreateRoomResponse, Box<dyn std::error::Error>> {
        let request = CreateRoomRequest {
            name: name.to_string(),
            host_id: host_id.to_string(),
            host_name: None,
        };

        let url = format!("{}/api/room/create", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("HTTP error: {}", error_text).into());
        }

        let text = response.text().await?;
        eprintln!("[HTTP] Response: {}", text);

        // Debug: print the JSON structure
        let room_response: CreateRoomResponse = serde_json::from_str(&text)
            .map_err(|e| format!("JSON parse error: {} - text: {}", e, text))?;

        eprintln!("[HTTP] Parsed room: {:?}", room_response.room);
        Ok(room_response)
    }

    /// 获取房间信息
    pub async fn get_room_info(
        &self,
        room_id: &str,
    ) -> Result<RoomInfo, Box<dyn std::error::Error>> {
        let url = format!("{}/api/room/{}/info", self.base_url, room_id);
        let response: reqwest::Response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("HTTP error: {}", error_text).into());
        }

        let room_info: RoomInfo = response.json().await?;
        Ok(room_info)
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let url = format!("{}/health", self.base_url);
        let response: reqwest::Response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err("Health check failed".into());
        }

        let health: serde_json::Value = response.json().await?;
        Ok(health)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check() {
        let client = HttpClient::new("lumino-collaborative-server.enderman-bm.workers.dev", 443);
        let health = client.health_check().await;
        assert!(health.is_ok());
    }
}
