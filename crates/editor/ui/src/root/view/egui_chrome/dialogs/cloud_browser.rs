//! 云文件浏览面板 / 断连提醒：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::{tr, trf};
use lumino_message::CloudAction;
use lumino_ui_egui::chrome::{
    CloudBrowserModel, CloudDeviceModel, CloudEntryActionKind, CloudEntryLabels, CloudEntryModel,
    CloudNoticeModel, CloudSavePanelModel,
};

use crate::Message;
use crate::state::cloud_state::CloudEntryUi;

impl crate::root::Root {
    /// 云浏览面板模型。
    pub(super) fn cloud_browser_model(&self) -> CloudBrowserModel {
        let state = &self.cloud;
        let devices = state
            .connections
            .iter()
            .enumerate()
            .map(|(index, conn)| CloudDeviceModel {
                index,
                label: format!(
                    "{} [{}] {}",
                    conn.name,
                    conn.protocol,
                    if conn.online {
                        tr!("在线")
                    } else {
                        tr!("离线")
                    }
                ),
                selected: state.selected_id.as_deref() == Some(conn.id.as_str()),
            })
            .collect();

        let placeholder = if state.busy && state.entries.is_empty() {
            Some(tr!("加载中...").to_string())
        } else if state.entries.is_empty() {
            Some(tr!("（空目录）").to_string())
        } else {
            None
        };

        let entries = state
            .entries
            .iter()
            .filter(|entry| {
                state
                    .filter
                    .as_ref()
                    .map(|ext| {
                        entry.is_dir || entry.name.to_lowercase().ends_with(&format!(".{ext}"))
                    })
                    .unwrap_or(true)
            })
            .map(|entry| entry_model(state, entry))
            .collect();

        CloudBrowserModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            devices,
            device_placeholder: tr!("选择云存储").to_string(),
            refresh: tr!("刷新").to_string(),
            disconnect: tr!("断开").to_string(),
            paste: if state.clipboard.as_ref().map(|c| c.is_cut).unwrap_or(false) {
                tr!("粘贴（剪切）").to_string()
            } else {
                tr!("粘贴").to_string()
            },
            clipboard: state.clipboard.is_some(),
            clear_clipboard: tr!("清空剪贴板").to_string(),
            back: tr!("← 返回").to_string(),
            current_path: state.current_path.clone(),
            new_folder_hint: tr!("新建文件夹名称").to_string(),
            new_folder_input: state.new_folder_input.clone(),
            new_folder: tr!("新建文件夹").to_string(),
            save_panel: state.save_mode.then(|| save_panel_model(self)),
            entries,
            placeholder,
            notice: state.notice.clone(),
            save_mode: state.save_mode,
            confirm_delete: tr!("确认删除？").to_string(),
            delete: tr!("删除").to_string(),
            cancel: tr!("取消").to_string(),
            rename_hint: tr!("新名称").to_string(),
            rename_input: state.rename_input.clone(),
            labels: CloudEntryLabels {
                download: tr!("下载").to_string(),
                copy: tr!("复制").to_string(),
                cut: tr!("剪切").to_string(),
                rename: tr!("重命名").to_string(),
                delete: tr!("删除").to_string(),
            },
            confirm: tr!("确定").to_string(),
        }
    }

    /// 断连提醒模型。
    pub(super) fn cloud_notice_model(&self) -> CloudNoticeModel {
        CloudNoticeModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("云存储提示").to_string(),
            message: self
                .cloud
                .alert_message
                .clone()
                .unwrap_or_else(|| tr!("云存储连接异常").to_string()),
            dismiss: tr!("知道了").to_string(),
        }
    }

    /// egui 云浏览 / 提醒动作 → 生产 `Message`。
    pub(crate) fn cloud_browser_action_message(
        &self,
        action: &lumino_ui_egui::cloud_browser::CloudBrowserAction,
    ) -> Option<Message> {
        use lumino_ui_egui::cloud_browser::CloudBrowserAction as Action;
        let action = match action {
            Action::SelectDevice(index) => {
                CloudAction::SelectStorage(self.cloud.connections.get(*index)?.id.clone())
            }
            Action::Refresh => CloudAction::Refresh,
            Action::Disconnect => CloudAction::Disconnect(self.cloud.selected_id.clone()?),
            Action::Paste => CloudAction::Paste,
            Action::ClearClipboard => CloudAction::ClearClipboard,
            Action::Back => CloudAction::Back,
            Action::EnterDir(path) => CloudAction::EnterDir(path.clone()),
            Action::NewFolderInputChanged(text) => CloudAction::NewFolderInputChanged(text.clone()),
            Action::NewFolder => CloudAction::NewFolder(self.cloud.new_folder_input.clone()),
            Action::SaveHere => CloudAction::SaveHere,
            Action::Download(path) => CloudAction::Download { path: path.clone() },
            Action::Copy { path, is_dir } => CloudAction::CopyEntry {
                path: path.clone(),
                is_dir: *is_dir,
            },
            Action::Cut { path, is_dir } => CloudAction::CutEntry {
                path: path.clone(),
                is_dir: *is_dir,
            },
            Action::StartRename(path) => CloudAction::StartRename(path.clone()),
            Action::RequestDelete { path, is_dir } => CloudAction::RequestDelete {
                path: path.clone(),
                is_dir: *is_dir,
            },
            Action::RenameInputChanged(text) => CloudAction::RenameInputChanged(text.clone()),
            Action::RenameConfirm => CloudAction::RenameConfirm,
            Action::RenameCancel => CloudAction::RenameCancel,
            Action::DeleteEntry { path, is_dir } => CloudAction::DeleteEntry {
                path: path.clone(),
                is_dir: *is_dir,
            },
            Action::DeleteCancel => CloudAction::DeleteCancel,
            Action::DismissNotice => CloudAction::DismissAlert,
        };
        Some(Message::Cloud(action))
    }
}

/// 保存目标区。
fn save_panel_model(root: &crate::root::Root) -> CloudSavePanelModel {
    let state = &root.cloud;
    let path = if state.current_path.is_empty() {
        "/"
    } else {
        state.current_path.as_str()
    };
    let title = match &state.pending_upload {
        Some(pending) => trf!(
            "将上传素材 {name} 到：{path}",
            name = pending.file_name,
            path = path
        ),
        None => trf!("将保存当前工程到：{path}", path = path),
    };
    CloudSavePanelModel {
        title,
        save: if state.busy {
            tr!("上传中...").to_string()
        } else {
            tr!("保存到此处").to_string()
        },
        busy: state.busy,
    }
}

/// 单个条目。
fn entry_model(
    state: &crate::state::cloud_state::CloudUiState,
    entry: &CloudEntryUi,
) -> CloudEntryModel {
    let is_renaming = state.renaming.as_deref() == Some(entry.path.as_str());
    let pending_delete = state
        .pending_delete
        .as_ref()
        .map(|(path, _, _)| path.as_str() == entry.path.as_str())
        .unwrap_or(false);

    let mut actions = Vec::new();
    if !state.save_mode {
        if !entry.is_dir {
            actions.push(CloudEntryActionKind::Download);
        }
        if entry.is_dir {
            actions.push(CloudEntryActionKind::Cut);
        } else {
            actions.push(CloudEntryActionKind::Copy);
            actions.push(CloudEntryActionKind::Cut);
        }
        actions.push(CloudEntryActionKind::Rename);
        actions.push(CloudEntryActionKind::Delete);
    }

    CloudEntryModel {
        path: entry.path.clone(),
        name: format!("{} {}", if entry.is_dir { "📁" } else { "📄" }, entry.name),
        is_dir: entry.is_dir,
        size: if entry.is_dir {
            String::new()
        } else {
            crate::state::cloud_state::format_size(entry.size)
        },
        renaming: is_renaming,
        pending_delete,
        actions,
    }
}
