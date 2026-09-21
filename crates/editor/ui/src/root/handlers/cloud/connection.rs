//! 云存储连接与目录浏览处理器
//!
//! 处理连接表单校验、连接发起与目录导航（选中/进入/返回/刷新）。

use lumino_message::CloudProtocolUi;

use super::*;

impl Root {
    /// 发起连接请求（校验必填项 + 发射 ConnectRequest 事件）
    pub(super) fn cloud_connect(&mut self) {
        if self.cloud.connecting {
            return;
        }
        // 校验必填项
        if self.cloud.address.trim().is_empty() {
            self.cloud.connect_error = Some("服务器地址不能为空".to_string());
            return;
        }
        // 解析端口（非法输入回退默认）
        let port = self
            .cloud
            .port
            .trim()
            .parse::<u16>()
            .unwrap_or_else(|_| self.cloud.protocol.default_port());
        let protocol = self.cloud.protocol;

        self.cloud.connecting = true;
        self.cloud.connect_error = None;
        event::emit(event::Event::cloud(cloud_event::Event::ConnectRequest {
            name: if self.cloud.name.trim().is_empty() {
                default_conn_name(protocol, &self.cloud.address)
            } else {
                self.cloud.name.trim().to_string()
            },
            protocol: protocol.as_str().to_string(),
            address: self.cloud.address.trim().to_string(),
            port,
            username: self.cloud.username.clone(),
            password: self.cloud.password.clone(),
        }));
    }

    /// 选中云存储连接：重置浏览状态并列出根目录
    pub(super) fn cloud_select_storage(&mut self, id: String) {
        if self.cloud.selected_id.as_deref() == Some(id.as_str()) {
            return;
        }
        self.cloud.selected_id = Some(id);
        self.cloud.current_path = String::new();
        self.cloud.entries.clear();
        self.cloud.notice = None;
        self.request_list_dir();
    }

    /// 进入子目录（防御：UI 只对目录行发此动作）
    pub(super) fn cloud_enter_dir(&mut self, path: String) {
        self.cloud.current_path = path;
        self.cloud.entries.clear();
        self.request_list_dir();
    }

    /// 返回上一级目录
    pub(super) fn cloud_back(&mut self) {
        let path = self.cloud.current_path.as_str();
        if path.is_empty() || path == "/" {
            self.cloud.current_path = String::new();
            return;
        }
        let trimmed = path.trim_end_matches('/');
        match trimmed.rfind('/') {
            Some(idx) => self.cloud.current_path = trimmed[..idx].to_string(),
            None => self.cloud.current_path = String::new(),
        }
        self.cloud.entries.clear();
        self.request_list_dir();
    }

    /// 请求列出当前选中连接的当前目录
    pub(super) fn request_list_dir(&mut self) {
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        self.cloud.busy = true;
        self.cloud.notice = None;
        event::emit(event::Event::cloud(cloud_event::Event::ListDirRequest {
            id,
            path: self.cloud.current_path.clone(),
        }));
    }
}

/// 默认连接名称（用户未填时按协议+地址生成）
fn default_conn_name(protocol: CloudProtocolUi, address: &str) -> String {
    format!("{} - {}", protocol.display_name(), address)
}
