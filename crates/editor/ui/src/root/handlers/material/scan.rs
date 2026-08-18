//! 素材库扫描与本地导入
//!
//! 2026-08-18 拆分：原 `root/handlers/material.rs`（803 行）按职责拆分，
//! 本模块承载素材列表后台扫描与本地文件导入。

use crate::root::Root;

impl Root {
    /// 开始后台扫描素材列表（内置 + 用户配置目录），完成后刷新面板
    pub(crate) fn start_material_scan(&mut self) {
        self.right_sidebar.materials.scanning = true;
        let user_dir = crate::right_sidebar::user_materials_dir();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let entries = crate::right_sidebar::scan_materials(&user_dir);
            let _ = tx.send(entries);
        });
        self.pending_material_scan = Some(rx);
    }

    /// 轮询素材扫描结果（后台扫描完成后刷新素材列表）
    pub(crate) fn poll_material_scan(&mut self) {
        let rx = match self.pending_material_scan.as_ref() {
            Some(rx) => rx,
            None => return,
        };
        let entries = match rx.try_recv() {
            Ok(entries) => entries,
            Err(_) => return, // Empty / Disconnected
        };
        self.pending_material_scan = None;
        self.right_sidebar.materials.scanning = false;
        self.right_sidebar.materials.entries = entries;
        tracing::info!(
            "素材库扫描完成：{} 个素材（内置 + 本地）",
            self.right_sidebar.materials.entries.len()
        );
    }

    /// 从本地选取 .lmmaterial 素材文件并导入
    ///
    /// 导入流程：文件对话框选择 → 复制到用户素材目录 → 重新扫描列表。
    pub(crate) fn import_material_from_local(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("选择要导入的素材文件")
            .add_filter("Lumino 素材", &["lmmaterial"])
            .add_filter("所有文件", &["*"])
            .pick_file()
        else {
            return;
        };

        // 校验素材格式（从 metadata 判断是否为素材文件）
        let valid_material = lumino_export::load_project(&path)
            .map(|p| p.metadata.is_material_file())
            .unwrap_or(false);
        if !valid_material {
            tracing::error!("导入失败：{} 不是素材文件（.lmmaterial）", path.display());
            self.toast.push(
                crate::toast::ToastLevel::Error,
                "素材导入失败：不是有效的素材文件",
            );
            return;
        }

        let user_dir = crate::right_sidebar::user_materials_dir();
        match crate::right_sidebar::copy_material_to_user_dir(&path, &user_dir) {
            Ok(dest) => {
                tracing::info!("素材已导入并复制到用户素材目录: {:?}", dest);
                self.toast
                    .push(crate::toast::ToastLevel::Success, "素材已导入");
                // 重新扫描列表
                self.start_material_scan();
            }
            Err(e) => {
                tracing::error!("素材复制失败: {e}");
                self.toast.push(
                    crate::toast::ToastLevel::Error,
                    "素材导入失败：复制文件出错",
                );
            }
        }
    }
}
