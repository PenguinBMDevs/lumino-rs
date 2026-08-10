//! # lumino-cloud — 云存储连接与文件管理
//!
//! 提供 FTP / SFTP / WebDAV 三种协议的统一封装：
//! - `model`：连接模型与状态
//! - `crypto`：密码 AES-256-GCM 加密（密钥编译期内置，不落盘）
//! - `config`：连接配置持久化（`cloud.json`，密码密文存储）
//! - `client`：统一 `CloudClient` trait（list/upload/download/rename/delete/move/mkdir）
//! - `ftp` / `sftp` / `webdav`：三种协议的客户端实现
//! - `manager`：`CloudManager` 连接池（自动连接、状态跟踪、操作分发）
//!
//! 本 crate 不依赖任何 UI 层，可独立测试与复用。

pub mod client;
pub mod config;
pub mod crypto;
mod dav_xml;
pub mod error;
pub mod ftp;
pub mod manager;
pub mod model;
pub mod sftp;
pub mod webdav;

pub use client::{CloudClient, create_client};
pub use config::{CloudConfigStore, save_config_to};
pub use error::{CloudError, Result};
pub use manager::CloudManager;
pub use model::{CloudConnection, CloudEntry, CloudProtocol, ConnState};
