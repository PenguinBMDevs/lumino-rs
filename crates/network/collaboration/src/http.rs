//! HTTP API 客户端
//!
//! 用于与服务器 HTTP API 交互（创建房间、获取房间信息等）

use crate::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

/// HTTP API 客户端
pub struct HttpClient {
    client: Client,
    base_url: String,
}

/// 分块上传流（DEBT-06 #123）。
///
/// 旧实现 `bytes.chunks(CHUNK).map(to_vec)` 会把整个载荷**再复制一份**（峰值 2×
/// 文件大小）。这里改为有界通道逐块生产：峰值 = 原载荷 + 通道容量 × 64 KiB。
struct ChunkStream {
    rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
}

impl futures::Stream for ChunkStream {
    type Item = std::result::Result<Vec<u8>, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx).map(|opt| opt.map(Ok))
    }
}

/// 创建房间请求
#[derive(Debug, Serialize)]
pub struct CreateRoomRequest {
    /// 房间名称
    pub name: String,
    /// 房主用户 ID
    #[serde(rename = "hostId")]
    pub host_id: String,
    /// 房主名称（可选）
    #[serde(rename = "hostName", skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,
}

/// 创建房间响应
#[derive(Debug, Deserialize)]
pub struct CreateRoomResponse {
    /// 是否创建成功
    pub success: bool,
    /// 创建的房间信息
    pub room: RoomInfo,
    /// WebSocket 连接地址
    #[serde(rename = "webSocketUrl")]
    pub web_socket_url: String,
}

/// 房间信息
#[derive(Debug, Deserialize, Clone)]
pub struct RoomInfo {
    /// 房间 ID
    pub id: String,
    /// 房间邀请码
    #[serde(rename = "inviteCode")]
    pub invite_code: String,
    /// 房间名称
    pub name: String,
    /// 房主用户 ID
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
            // DEBT-06 #123：协作 HTTP 必须有超时——旧实现 `Client::new()` 无任何
            // 超时，服务器假死会让连接/上传永久挂起（UI 停在"连接中/上传中"）。
            // 用 `read_timeout`（两次读之间的空闲上限）而非总超时：
            // 大工程上传的总时长不该被上限误杀，但卡死会按空闲超时失败。
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .read_timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|e| {
                    tracing::warn!("协作 HTTP 客户端超时配置构建失败，回退默认配置: {e}");
                    Client::new()
                }),
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

    /// 上传房间工程文件（host 路径）。
    ///
    /// `POST /api/room/{code}/project?name=<urlencoded>&hash=<hex>`，
    /// body 为原始 `.lmpj` 字节（octet-stream）。返回服务器 JSON 响应。
    ///
    /// `on_progress` 可选：传入 `(已发送字节, 总字节)` 回调用于驱动上传进度条。
    /// 内部以 64 KiB 分块流式上传，逐块上报进度（避免一次性把大文件读进内存且无进度）。
    pub async fn upload_room_project(
        &self,
        code: &str,
        name: &str,
        hash: &str,
        bytes: Vec<u8>,
        on_progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    ) -> Result<serde_json::Value> {
        let total = bytes.len() as u64;
        let url = format!("{}/api/room/{}/project", self.base_url, code);

        // 分块流式上传，逐块上报已发送字节数（用于进度条）。
        // DEBT-06 #123：有界通道逐块生产（峰值 ≈ 原载荷 + 2×64KiB），
        // 旧实现预先把全部块各复制一份到 Vec（峰值 2× 文件大小）。
        const CHUNK: usize = 64 * 1024;
        let body = match on_progress {
            Some(cb) => {
                let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
                tokio::spawn(async move {
                    let mut sent: u64 = 0;
                    for chunk in bytes.chunks(CHUNK) {
                        if tx.send(chunk.to_vec()).await.is_err() {
                            // 请求提前结束（取消/错误）：停止生产，释放载荷
                            return;
                        }
                        sent += chunk.len() as u64;
                        cb(sent, total);
                    }
                });
                reqwest::Body::wrap_stream(ChunkStream { rx })
            }
            None => reqwest::Body::from(bytes),
        };

        let response = self
            .client
            .post(&url)
            .query(&[("name", name), ("hash", hash)])
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(crate::CollaborationError::Other(format!(
                "HTTP error: {}",
                text
            )));
        }

        serde_json::from_str(&text).map_err(|e| {
            crate::CollaborationError::Other(format!("JSON parse error: {} - text: {}", e, text))
        })
    }

    /// 下载房间工程文件（joiner 路径）。
    ///
    /// `GET /api/room/{code}/project` → 原始 `.lmpj` 字节。
    /// 服务器返回 404 时返回 `Err`（调用方据此跳过同步）。
    pub async fn download_room_project(&self, code: &str) -> Result<Vec<u8>> {
        let url = format!("{}/api/room/{}/project", self.base_url, code);
        let response = self.client.get(&url).send().await?;

        if response.status().as_u16() == 404 {
            return Err(crate::CollaborationError::Other(
                "room project not found (404)".to_string(),
            ));
        }
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(crate::CollaborationError::Other(format!(
                "HTTP error: {}",
                text
            )));
        }

        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
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
