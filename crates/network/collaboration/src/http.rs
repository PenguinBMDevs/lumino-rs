//! HTTP API 客户端
//!
//! 用于与服务器 HTTP API 交互（创建房间、获取房间信息等）

use crate::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

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
    pub async fn create_room(&self, name: &str, host_id: &str) -> Result<CreateRoomResponse> {
        let request = CreateRoomRequest {
            name: name.to_string(),
            host_id: host_id.to_string(),
            host_name: None,
        };

        let url = format!("{}/api/room/create", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            return Err(crate::CollaborationError::Other(format!(
                "HTTP error: {}",
                text
            )));
        }

        debug!(response = %text, "[HTTP] Response");

        // Debug: print the JSON structure
        let room_response: CreateRoomResponse = serde_json::from_str(&text).map_err(|e| {
            crate::CollaborationError::Other(format!("JSON parse error: {} - text: {}", e, text))
        })?;

        debug!(?room_response.room, "[HTTP] Parsed room");
        Ok(room_response)
    }

    /// 获取房间信息
    pub async fn get_room_info(&self, room_id: &str) -> Result<RoomInfo> {
        let url = format!("{}/api/room/{}/info", self.base_url, room_id);
        let response: reqwest::Response = self.client.get(&url).send().await?;

        let status = response.status();
        let error_text = response.text().await?;

        if !status.is_success() {
            return Err(crate::CollaborationError::Other(format!(
                "HTTP error: {}",
                error_text
            )));
        }

        let room_info: RoomInfo = serde_json::from_str(&error_text)?;
        Ok(room_info)
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<serde_json::Value> {
        let url = format!("{}/health", self.base_url);
        let response: reqwest::Response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(crate::CollaborationError::Other(
                "Health check failed".to_string(),
            ));
        }

        let health: serde_json::Value = response.json().await?;
        Ok(health)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 健康检查测试 —— 需要外网连接，默认忽略。
    /// 运行: `cargo test test_health_check -- --ignored`
    #[tokio::test]
    #[ignore = "需要外部协作服务器"]
    async fn test_health_check() {
        let client = HttpClient::new("lumino-collaborative-server.enderman-bm.workers.dev", 443);
        let health = client.health_check().await;
        assert!(health.is_ok());
    }

    #[test]
    fn test_http_client_base_url() {
        let client = HttpClient::new("example.com", 443);
        assert_eq!(client.base_url, "https://example.com");

        let client = HttpClient::new("example.com", 80);
        assert_eq!(client.base_url, "http://example.com");

        let client = HttpClient::new("example.com", 3000);
        assert_eq!(client.base_url, "http://example.com:3000");
    }

    #[test]
    fn test_create_room_request_serialization() {
        let req = CreateRoomRequest {
            name: "测试房间".to_string(),
            host_id: "user_123".to_string(),
            host_name: Some("测试用户".to_string()),
        };
        let json = serde_json::to_string(&req).expect("序列化创建房间请求失败");
        assert!(json.contains("\"name\":\"测试房间\""));
        assert!(json.contains("\"hostId\":\"user_123\""));
        assert!(json.contains("\"hostName\":\"测试用户\""));
    }
}
