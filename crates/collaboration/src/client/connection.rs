//! 协作客户端连接管理

use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info};

use crate::HEARTBEAT_INTERVAL_MS;
use crate::client::{
    ClientConfig, ClientMessage, ClientState, CollaborationClient, CollaborationSession,
    EventCallback, ServerMessage, handle_server_message,
};

impl CollaborationClient {
    /// 连接到服务器
    pub async fn connect(
        &mut self,
        host: Option<String>,
        port: Option<u16>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let host = host.unwrap_or_else(|| self.config.server_host.clone());
        let port = port.unwrap_or(self.config.server_port);

        // 构建 WebSocket URL
        let protocol = if host.ends_with(".workers.dev") || host.ends_with(".dpdns.org") {
            "wss"
        } else {
            "ws"
        };

        // 对于标准 HTTPS 端口 (443)，不包含端口号
        let ws_url = if port == 443 {
            format!("{}://{}/ws", protocol, host)
        } else {
            format!("{}://{}:{}/ws", protocol, host, port)
        };

        info!("连接到服务器: {}", ws_url);

        *self.state.write().await = ClientState::Connecting;

        // 建立WebSocket连接，设置15秒超时
        let connect_future = connect_async(&ws_url);
        let timeout_duration = Duration::from_secs(15);

        info!("开始连接，超时时间: {}秒", timeout_duration.as_secs());

        let (ws_stream, _) = match tokio::time::timeout(timeout_duration, connect_future).await {
            Ok(result) => {
                info!("连接尝试完成");
                result?
            }
            Err(_) => {
                error!("连接超时（{}秒）", timeout_duration.as_secs());
                return Err(format!("连接超时（{}秒）", timeout_duration.as_secs()).into());
            }
        };

        info!("WebSocket连接成功");

        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));
        *self.state.write().await = ClientState::Connected;

        // 发送认证消息
        let auth_msg = ClientMessage::Auth {
            username: self.config.username.clone(),
        };
        let auth_json = serde_json::to_string(&auth_msg)?;
        info!("发送认证消息: {}", auth_json);
        write
            .lock()
            .await
            .send(Message::Text(auth_json.into()))
            .await?;
        info!("认证消息已发送");
        *self.state.write().await = ClientState::Authenticating;

        // 等待认证响应
        info!("等待认证响应...");
        self.handle_auth_response(&mut read, &write).await?;

        // 启动消息处理循环
        self.start_message_loop(read, write).await;

        Ok(())
    }

    /// 处理认证响应
    async fn handle_auth_response(
        &mut self,
        read: &mut tokio_tungstenite::WebSocketStream,
        write: &Arc<Mutex<tokio_tungstenite::WebSocketStream>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use tokio_tungstenite::tungstenite::protocol::Message;

        loop {
            match read.next().await {
                Some(Ok(Message::Text(text))) => {
                    info!("收到认证响应: {}", text);
                    let response: ServerMessage = match serde_json::from_str(&text) {
                        Ok(r) => r,
                        Err(e) => {
                            error!("解析认证响应失败: {}\n响应内容: {}", e, text);
                            continue;
                        }
                    };
                    match response {
                        ServerMessage::AuthSuccess {
                            user_id,
                            invite_code,
                        } => {
                            info!("认证成功: user_id={}", user_id);
                            *self.state.write().await = ClientState::Authenticated;

                            let mut session = self.session.write().await;
                            session.current_user_id = Some(user_id.clone());
                            session.invite_code = Some(invite_code.clone());
                            drop(session);

                            self.emit_event(crate::client::CollaborationEvent::Authenticated {
                                user_id,
                                invite_code,
                            });
                            break;
                        }
                        ServerMessage::AuthError { error } => {
                            error!("认证失败: {}", error);
                            *self.state.write().await = ClientState::Error;
                            return Err(error.into());
                        }
                        _ => {
                            info!("在认证阶段收到其他消息，忽略");
                            continue;
                        }
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    error!("连接在认证前被关闭: {:?}", frame);
                    return Err("连接被关闭".into());
                }
                Some(Err(e)) => {
                    error!("WebSocket 错误: {}", e);
                    return Err(e.into());
                }
                Some(Ok(_)) => {
                    continue;
                }
                None => {
                    error!("连接在认证前断开");
                    return Err("连接断开".into());
                }
            }
        }
        Ok(())
    }

    /// 启动消息处理循环
    async fn start_message_loop(
        &mut self,
        mut read: tokio_tungstenite::WebSocketStream,
        write: Arc<Mutex<tokio_tungstenite::WebSocketStream>>,
    ) {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let state = self.state.clone();
        let session = self.session.clone();
        let message_rx = self.message_rx.clone();
        let event_callback = self.event_callback.clone();

        tokio::spawn(async move {
            info!("消息处理循环已启动");
            let mut heartbeat = interval(Duration::from_millis(HEARTBEAT_INTERVAL_MS));

            loop {
                tokio::select! {
                    msg = read.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                if let Err(e) = handle_server_message(
                                    &text,
                                    &state,
                                    &session,
                                    event_callback.clone()
                                ).await {
                                    error!("处理消息失败: {} - {}", e, text);
                                }
                            }
                            Some(Ok(Message::Close(_))) => {
                                *state.write().await = ClientState::Disconnected;
                                break;
                            }
                            Some(Err(e)) => {
                                error!("WebSocket错误: {}", e);
                                *state.write().await = ClientState::Error;
                            }
                            None => {
                                *state.write().await = ClientState::Disconnected;
                                break;
                            }
                            _ => {}
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("收到关闭信号");
                        break;
                    }
                }
            }
            info!("消息处理循环已结束");
        });
    }
}
