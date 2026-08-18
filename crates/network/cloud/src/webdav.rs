//! WebDAV 客户端实现（基于 reqwest_dav）
//!
//! 路径规范：本模块内统一使用**相对路径**（如 `/dav/dir/file.txt`），
//! 因为 reqwest_dav 内部以 `host + path` 拼接请求 URL，传完整 URL 会出错。
//! 服务器返回的 href 通过 `normalize_href` 规范化为相对路径。
//!
//! 该模块已拆分为以下子模块：
//! - `path`: 路径编码与规范化工具（normalize_href / encode_remote_path / build_host 等）
//! - `error`: 响应错误分级诊断与诊断日志辅助（status_error / html_response_error 等）

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use reqwest_dav::types::Auth;
use reqwest_dav::types::list_cmd::ListEntity;
use reqwest_dav::{Client as DavClient, ClientBuilder, Depth};

use crate::client::{CloudClient, basename, join_remote, normalize_remote};
use crate::crypto::decrypt;
use crate::dav_xml::{is_html_response, parse_list_multi_status, response_preview};
use crate::error::{CloudError, Result};
use crate::model::{CloudConnection, CloudEntry};

mod error;
mod path;

use error::{content_type_is_html, dav_err, format_headers, html_response_error, status_error};
use path::{build_host, encode_remote_path, is_self, normalize_href};

/// 连接/请求超时（秒）
const CONNECT_TIMEOUT_SECS: u64 = 10;
const REQUEST_TIMEOUT_SECS: u64 = 60;

/// WebDAV 客户端
pub struct WebdavClient {
    client: Option<DavClient>,
    /// 服务器 host（完整 URL，如 `https://example.com/dav`）
    host: String,
}

impl WebdavClient {
    /// 创建空客户端
    pub fn new() -> Self {
        Self {
            client: None,
            host: String::new(),
        }
    }
}

#[async_trait]
impl CloudClient for WebdavClient {
    async fn connect(&mut self, conn: &CloudConnection) -> Result<()> {
        let password = decrypt(&conn.password_encrypted)?;
        if conn.address.trim().is_empty() {
            return Err(CloudError::Config("WebDAV 地址不能为空".into()));
        }
        // 地址 + 独立端口字段（端口未填时默认 80；地址已含端口时以地址为准）
        let host = build_host(&conn.address, conn.port);

        let agent = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            // WebDAV 协议必须禁用自动重定向：
            // 1) 重定向会触发 reqwest 方法降级（PROPFIND → GET），协议语义被破坏，
            //    服务器（如群晖 NAS）对未认证 PROPFIND 返回 302 登录页，跟随后
            //    拿到的是 DSM 登录页 HTML 而非 WebDAV 响应
            // 2) 标准 WebDAV 客户端均不跟随重定向
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| CloudError::Connect(format!("创建 HTTP 客户端失败: {e}")))?;

        let auth = if conn.username.is_empty() {
            Auth::Anonymous
        } else {
            Auth::Basic(conn.username.clone(), password)
        };

        let client = ClientBuilder::new()
            .set_agent(agent)
            .set_host(host.clone())
            .set_auth(auth)
            .build()
            .map_err(dav_err)?;

        self.client = Some(client);
        self.host = host;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.client.take();
        Ok(())
    }

    async fn list_dir(&mut self, path: &str) -> Result<Vec<CloudEntry>> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        // 统一规范化为绝对路径（消除 ./ 残留）；请求用编码形式，is_self 比较用原始形式
        let target_raw = normalize_remote(path);
        let target = encode_remote_path(&target_raw);

        // 使用 list_raw 拿原始响应，绕开 reqwest_dav 内置的严格 XML 解析：
        // 部分服务器会在 PROPFIND 响应中输出 `&nbsp;` 等 HTML 实体，
        // 内置 serde_xml_rs 解析直接报 `Unexpected entity: nbsp`，导致列表整体失败。
        let resp = client
            .list_raw(&target, Depth::Number(1))
            .await
            .map_err(dav_err)?;
        if !resp.status().is_success() {
            return Err(status_error(resp.status()));
        }
        // 先取 Content-Type 与完整响应头（text() 消费响应后无法再读 header），
        // 与内容嗅探共同判断服务器是否返回了 HTML 页面
        let html_by_content_type = content_type_is_html(
            resp.headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
        );
        let headers_snapshot = format_headers(resp.headers());
        let text = resp
            .text()
            .await
            .map_err(|e| CloudError::Protocol(format!("读取 PROPFIND 响应失败: {e}")))?;
        // 服务器返回 HTML 页面（登录页/代理错误页）而非 WebDAV 响应：
        // 状态码可能是 200，但内容解析必炸。命中时响应头与 HTML 源码已由
        // html_response_error 打到 tracing 日志，错误信息回传实际连接端口
        if html_by_content_type || is_html_response(&text) {
            let port = reqwest::Url::parse(&self.host)
                .ok()
                .and_then(|u| u.port_or_known_default())
                .unwrap_or(80);
            return Err(html_response_error(&headers_snapshot, &text, port));
        }
        let parsed = parse_list_multi_status(&text).map_err(|e| {
            CloudError::Protocol(format!(
                "解析 PROPFIND 响应失败: {e}（响应预览: {}）",
                response_preview(&text)
            ))
        })?;
        let entities: Vec<ListEntity> = parsed
            .responses
            .into_iter()
            .map(ListEntity::try_from)
            .collect::<std::result::Result<_, _>>()
            .map_err(dav_err)?;

        let mut entries = Vec::new();
        for entity in entities {
            match entity {
                ListEntity::File(f) => {
                    if is_self(&f.href, &target_raw) {
                        continue;
                    }
                    let remote = normalize_href(&f.href);
                    entries.push(CloudEntry {
                        name: basename(&remote),
                        path: normalize_remote(&remote),
                        is_dir: false,
                        size: f.content_length.max(0) as u64,
                        modified: Some(f.last_modified.timestamp().max(0) as u64),
                    });
                }
                ListEntity::Folder(f) => {
                    if is_self(&f.href, &target_raw) {
                        continue;
                    }
                    let remote = normalize_href(&f.href);
                    entries.push(CloudEntry {
                        name: basename(&remote),
                        path: normalize_remote(&remote),
                        is_dir: true,
                        size: 0,
                        modified: Some(f.last_modified.timestamp().max(0) as u64),
                    });
                }
            }
        }
        Ok(entries)
    }

    async fn upload_file(&mut self, local: &Path, remote_path: &str) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let bytes = std::fs::read(local).map_err(CloudError::Io)?;
        let remote_path = encode_remote_path(&normalize_remote(remote_path));
        client
            .put(&remote_path, bytes)
            .await
            .map_err(|e| CloudError::Protocol(format!("上传失败 {remote_path}: {e}")))?;
        Ok(())
    }

    async fn download_file(&mut self, remote_path: &str, local: &Path) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let remote_path = encode_remote_path(&normalize_remote(remote_path));
        let resp = client
            .get(&remote_path)
            .await
            .map_err(|e| CloudError::Protocol(format!("下载失败 {remote_path}: {e}")))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CloudError::Protocol(format!("读取下载数据失败: {e}")))?;
        std::fs::write(local, &bytes).map_err(CloudError::Io)?;
        Ok(())
    }

    async fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let from = encode_remote_path(&normalize_remote(from));
        let to = encode_remote_path(&normalize_remote(to));
        client
            .mv(&from, &to)
            .await
            .map_err(|e| CloudError::Protocol(format!("重命名失败 {from} → {to}: {e}")))?;
        Ok(())
    }

    async fn delete(&mut self, path: &str, _is_dir: bool) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let path = encode_remote_path(&normalize_remote(path));
        client
            .delete(&path)
            .await
            .map_err(|e| CloudError::Protocol(format!("删除失败 {path}: {e}")))?;
        Ok(())
    }

    async fn move_file(&mut self, from: &str, to_dir: &str) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        // basename 必须先于编码提取，避免已编码的 %20 被二次编码为 %25
        let from_norm = normalize_remote(from);
        let from = encode_remote_path(&from_norm);
        let to = encode_remote_path(&normalize_remote(
            join_remote(to_dir, &basename(&from_norm)).as_str(),
        ));
        client
            .mv(&from, &to)
            .await
            .map_err(|e| CloudError::Protocol(format!("移动失败 {from} → {to}: {e}")))?;
        Ok(())
    }

    async fn create_dir(&mut self, path: &str) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| CloudError::NotConnected("WebDAV 未连接".into()))?;
        let path = encode_remote_path(&normalize_remote(path));
        client
            .mkcol(&path)
            .await
            .map_err(|e| CloudError::Protocol(format!("创建目录失败 {path}: {e}")))?;
        Ok(())
    }
}

impl Default for WebdavClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
