//! 云存储文件操作处理器
//!
//! 处理远程条目的下载、新建、上传保存与复制/剪切/粘贴/重命名/删除。

use crate::state::cloud_state::CloudClipboard;

use super::*;

impl Root {
    /// 下载远程文件（素材过滤器决定目标：素材库 or 导入目录）
    pub(super) fn cloud_download(&mut self, path: String) {
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        let target = if self.cloud.filter.as_deref() == Some("lmmaterial") {
            cloud_event::DownloadTarget::Material
        } else {
            cloud_event::DownloadTarget::Import
        };
        self.cloud.busy = true;
        self.cloud.notice = None;
        event::emit(event::Event::cloud(cloud_event::Event::DownloadRequest {
            id,
            remote_path: path,
            target,
        }));
    }

    /// 新建文件夹（校验名称 + 发射 NewFolderRequest 事件）
    pub(super) fn cloud_new_folder(&mut self, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.cloud.notice = Some("文件夹名称不能为空".to_string());
            return;
        }
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        self.cloud.busy = true;
        self.cloud.new_folder_input.clear();
        event::emit(event::Event::cloud(cloud_event::Event::NewFolderRequest {
            id,
            parent: self.cloud.current_path.clone(),
            name,
        }));
    }

    /// 保存到当前目录：有素材上传待办 → 上传素材；否则上传工程归档
    pub(super) fn cloud_save_here(&mut self) {
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        self.cloud.busy = true;
        self.cloud.notice = None;
        // 素材上传待办存在 → 上传素材文件（素材库右键"上传到云"）；
        // 否则上传当前工程归档（文件菜单"保存到云"）。
        if let Some(pending) = self.cloud.pending_upload.take() {
            event::emit(event::Event::cloud(
                cloud_event::Event::UploadMaterialRequest {
                    id,
                    dir_path: self.cloud.current_path.clone(),
                    local_path: pending.local_path,
                    file_name: pending.file_name,
                },
            ));
        } else {
            event::emit(event::Event::cloud(
                cloud_event::Event::SaveToCloudRequest {
                    id,
                    dir_path: self.cloud.current_path.clone(),
                },
            ));
        }
    }

    /// 复制条目到剪贴板（目录复制暂不支持）
    pub(super) fn cloud_copy_entry(&mut self, path: String, is_dir: bool) {
        if is_dir {
            self.cloud.notice = Some("复制目录暂不支持，请使用剪切移动".to_string());
            return;
        }
        self.cloud.clipboard = Some(CloudClipboard::new(path.clone(), false, false));
        self.cloud.notice = Some(format!("已复制：{}", basename_of(&path)));
    }

    /// 剪切条目到剪贴板
    pub(super) fn cloud_cut_entry(&mut self, path: String, is_dir: bool) {
        self.cloud.clipboard = Some(CloudClipboard::new(path.clone(), is_dir, true));
        self.cloud.notice = Some(format!("已剪切：{}", basename_of(&path)));
    }

    /// 粘贴剪贴板条目（复制/移动请求）
    pub(super) fn cloud_paste(&mut self) {
        let Some(clip) = self.cloud.clipboard.clone() else {
            self.cloud.notice = Some("剪贴板为空".to_string());
            return;
        };
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        event::emit(event::Event::cloud(cloud_event::Event::CopyRequest {
            id,
            from: clip.source_path,
            to_dir: self.cloud.current_path.clone(),
            is_cut: clip.is_cut,
        }));
    }

    /// 请求删除：进入行内确认态（不立即删除）
    pub(super) fn cloud_request_delete(&mut self, path: String, is_dir: bool) {
        self.cloud.pending_delete = Some((path.clone(), is_dir, basename_of(&path)));
        self.cloud.notice = None;
    }

    /// 确认删除（由行内确认态触发）
    pub(super) fn cloud_delete_entry(&mut self, path: String, is_dir: bool) {
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        self.cloud.busy = true;
        self.cloud.pending_delete = None;
        self.cloud.notice = None;
        event::emit(event::Event::cloud(cloud_event::Event::DeleteRequest {
            id,
            path,
            is_dir,
        }));
    }

    /// 开始重命名：进入行内编辑态
    pub(super) fn cloud_start_rename(&mut self, path: String) {
        self.cloud.renaming = Some(path.clone());
        self.cloud.rename_input = basename_of(&path);
        self.cloud.notice = None;
    }

    /// 确认重命名：目标路径 = 源目录 + 新名称
    pub(super) fn cloud_rename_confirm(&mut self) {
        let Some(from) = self.cloud.renaming.clone() else {
            return;
        };
        let new_name = self.cloud.rename_input.trim().to_string();
        if new_name.is_empty() {
            self.cloud.notice = Some("名称不能为空".to_string());
            return;
        }
        let Some(id) = self.cloud.selected_id.clone() else {
            self.cloud.notice = Some("未选择云存储".to_string());
            return;
        };
        // 目标路径 = 源目录 + 新名称
        let parent = match from.rfind('/') {
            Some(0) => String::new(),
            Some(idx) => from[..idx].to_string(),
            None => String::new(),
        };
        let to = if parent.is_empty() {
            format!("/{new_name}")
        } else {
            format!("{parent}/{new_name}")
        };
        self.cloud.renaming = None;
        self.cloud.notice = None;
        event::emit(event::Event::cloud(cloud_event::Event::RenameRequest {
            id,
            from,
            to,
        }));
    }
}

/// 从远程完整路径提取条目名（与 cloud 客户端 basename 语义一致）
fn basename_of(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(idx) => trimmed[idx + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basename_of() {
        assert_eq!(basename_of("/Moyingjun/file.lmpj"), "file.lmpj");
        assert_eq!(basename_of("/file.lmpj"), "file.lmpj");
        assert_eq!(basename_of("file.lmpj"), "file.lmpj");
        assert_eq!(basename_of("/Moyingjun/dir/"), "dir");
        assert_eq!(
            basename_of("/Moyingjun/Parallel Unit.lmpj"),
            "Parallel Unit.lmpj"
        );
    }
}
