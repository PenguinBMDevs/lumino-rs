//! 素材上传到云
//!
//! 2026-08-18 拆分：原 `root/handlers/material.rs`（803 行）按职责拆分，
//! 本模块承载素材上传到云流程（设置上传待办 + 打开云存储面板）。

use crate::root::Root;
use crate::toast::ToastLevel;

impl Root {
    /// 上传素材到云：设置待办并打开云存储文件管理面板（选择上传位置）
    ///
    /// - 无在线连接：runner 分流弹出云存储连接面板引导配置；
    /// - 有在线连接：打开云浏览面板（保存模式），用户选择目录后点"保存到此处"。
    ///
    /// 仅用户素材可上传（调用方已做防御门；内置素材无磁盘路径，不可上传）。
    pub(crate) fn upload_material_to_cloud(&mut self, index: usize) {
        let Some(entry) = self.right_sidebar.materials.entries.get(index) else {
            return;
        };
        if !entry.valid {
            self.toast.push(ToastLevel::Error, "素材无效，无法上传");
            return;
        }
        let Some(path) = &entry.path else {
            self.toast.push(ToastLevel::Error, "内置素材不可上传");
            return;
        };
        // 远程文件名：素材显示名 + 扩展名；过滤路径分隔符（防止创建子路径）
        let file_name = format!("{}.lmmaterial", entry.name.replace(['/', '\\'], "_"));

        // 设置上传待办（云浏览面板"保存到此处"时消费）
        self.cloud.pending_upload = Some(crate::state::cloud_state::PendingUpload {
            local_path: path.to_string_lossy().into_owned(),
            file_name,
        });
        // 云入口分流：无连接 → runner 弹出连接面板；已连接 → 浏览面板（保存模式）
        crate::event::emit(crate::event::Event::cloud(
            crate::event::cloud::Event::OpenCloudPanel {
                intent: "material_upload".to_string(),
            },
        ));
        tracing::info!("素材 {} 上传到云流程已启动", entry.name);
    }
}
