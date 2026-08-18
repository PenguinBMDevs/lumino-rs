//! 素材右键菜单：打开 / 关闭 / 项点击 / 重命名 / 删除确认
//!
//! 2026-08-18 拆分：原 `root/handlers/material.rs`（803 行）按职责拆分，
//! 本模块承载素材右键菜单及其重命名 / 删除确认流程。

use lumino_message::MaterialContextMenuItem;

use crate::root::Root;
use crate::toast::ToastLevel;

impl Root {
    // ── 素材右键菜单 ──

    /// 处理打开素材右键菜单
    pub(crate) fn open_material_context_menu(&mut self, index: usize) {
        if self.right_sidebar.materials.entries.get(index).is_none() {
            return;
        }
        // 互斥：关闭其他浮动状态
        self.right_sidebar.materials.add_menu_open = false;
        self.right_sidebar.materials.renaming_material = None;
        self.right_sidebar.materials.pending_delete = None;
        // 快照当前鼠标位置（面板局部坐标）作为菜单弹出位置；
        // 菜单打开期间该位置冻结，不跟随鼠标移动
        self.right_sidebar.materials.context_menu_pos =
            self.right_sidebar.materials.context_cursor_pos;
        self.right_sidebar.materials.context_menu_target = Some(index);
    }

    /// 处理关闭素材右键菜单
    pub(crate) fn close_material_context_menu(&mut self) {
        self.right_sidebar.materials.context_menu_target = None;
        self.right_sidebar.materials.context_menu_pos = None;
    }

    /// 处理点击素材右键菜单项
    pub(crate) fn handle_material_context_menu_item_clicked(
        &mut self,
        index: usize,
        item: MaterialContextMenuItem,
    ) {
        self.right_sidebar.materials.context_menu_target = None;
        match item {
            MaterialContextMenuItem::Rename => {
                // 仅用户素材可重命名（内置素材的按钮已置灰，此处为防御）
                if let Some(entry) = self.right_sidebar.materials.entries.get(index)
                    && entry.path.is_some()
                {
                    self.right_sidebar.materials.renaming_material =
                        Some((index, entry.name.clone()));
                }
            }
            MaterialContextMenuItem::Delete => {
                // 仅用户素材可删除（内置素材的按钮已置灰，此处为防御）
                if let Some(entry) = self.right_sidebar.materials.entries.get(index)
                    && entry.path.is_some()
                {
                    // 进入删除确认态：主窗口叠加覆盖层弹窗展示确认卡片
                    self.right_sidebar.materials.pending_delete = Some(index);
                    self.right_sidebar.materials.pending_delete_name = Some(entry.name.clone());
                }
            }
            MaterialContextMenuItem::UploadToCloud => {
                // 仅用户素材可上传到云（内置素材为程序资产，按钮已置灰，此处为防御）
                if let Some(entry) = self.right_sidebar.materials.entries.get(index)
                    && entry.path.is_some()
                {
                    self.upload_material_to_cloud(index);
                }
            }
        }
    }

    // ── 素材重命名 ──

    /// 处理素材重命名输入变化
    pub(crate) fn handle_material_rename_input_changed(&mut self, value: String) {
        if let Some((_, buffer)) = &mut self.right_sidebar.materials.renaming_material {
            *buffer = value;
        }
    }

    /// 处理取消素材重命名
    pub(crate) fn cancel_material_rename(&mut self) {
        self.right_sidebar.materials.renaming_material = None;
    }

    /// 处理确认素材重命名
    ///
    /// 流程：加载工程 → 写入新名称（文件 + metadata 同步）→ 删除旧文件 → 重新扫描。
    /// 与素材显示名规则一致：`metadata.project.name` 优先，故必须双改。
    pub(crate) fn confirm_material_rename(&mut self) {
        let Some((index, buffer)) = self.right_sidebar.materials.renaming_material.take() else {
            return;
        };
        let new_name = buffer.trim().replace(['/', '\\'], "_");
        if new_name.is_empty() || new_name == "." || new_name == ".." {
            self.toast.push(ToastLevel::Error, "素材名称不能为空");
            return;
        }
        let Some(entry) = self.right_sidebar.materials.entries.get(index) else {
            return;
        };
        let Some(old_path) = &entry.path else {
            self.toast.push(ToastLevel::Error, "内置素材不可重命名");
            return;
        };
        // 新路径 = 用户素材目录 / 新名称.lmmaterial（与导入落点一致）
        let user_dir = crate::right_sidebar::user_materials_dir();
        let new_path = user_dir.join(format!("{new_name}.lmmaterial"));
        if new_path.exists() {
            self.toast.push(ToastLevel::Error, "已存在同名素材");
            return;
        }
        // 加载工程 → 以新名称重新保存（同步 metadata.project.name）→ 删除旧文件
        match lumino_export::load_project(old_path) {
            Ok(project) => {
                if let Err(e) = lumino_export::save_material(&project, &new_name, &new_path) {
                    self.toast
                        .push(ToastLevel::Error, format!("素材重命名失败：{e}"));
                    return;
                }
                if let Err(e) = std::fs::remove_file(old_path) {
                    // 新文件已保存；旧文件删除失败会导致列表出现两份，提示用户处理
                    tracing::warn!("素材重命名后旧文件删除失败: {e}");
                    self.toast.push(
                        ToastLevel::Error,
                        format!("素材已保存为新名称，但旧文件删除失败：{e}"),
                    );
                } else {
                    self.toast.push(ToastLevel::Success, "素材已重命名");
                }
                self.start_material_scan();
            }
            Err(e) => {
                self.toast.push(
                    ToastLevel::Error,
                    format!("素材重命名失败：无法读取原文件 {e}"),
                );
            }
        }
    }

    // ── 素材删除确认 ──

    /// 处理取消素材删除确认
    ///
    /// 覆盖层确认卡片的[取消]按钮/点击遮罩调用，清除确认态。
    pub(crate) fn cancel_material_delete(&mut self) {
        self.right_sidebar.materials.pending_delete = None;
        self.right_sidebar.materials.pending_delete_name = None;
    }

    /// 处理确认素材删除（删除本地文件并重新扫描）
    ///
    /// `index` 必须与当前待确认索引一致（防御：只允许确认当前卡片对应的素材项）。
    /// 覆盖层确认卡片的[删除]按钮调用。
    pub(crate) fn confirm_material_delete(&mut self, index: usize) {
        if self.right_sidebar.materials.pending_delete != Some(index) {
            return;
        }
        self.right_sidebar.materials.pending_delete = None;
        self.right_sidebar.materials.pending_delete_name = None;
        let Some(entry) = self.right_sidebar.materials.entries.get(index) else {
            return;
        };
        let Some(path) = &entry.path else {
            self.toast.push(ToastLevel::Error, "内置素材不可删除");
            return;
        };
        match std::fs::remove_file(path) {
            Ok(()) => {
                tracing::info!("素材已删除: {}", path.display());
                self.toast.push(ToastLevel::Success, "素材已删除");
                self.start_material_scan();
            }
            Err(e) => {
                tracing::error!("素材删除失败: {e}");
                self.toast
                    .push(ToastLevel::Error, format!("素材删除失败：{e}"));
            }
        }
    }
}
