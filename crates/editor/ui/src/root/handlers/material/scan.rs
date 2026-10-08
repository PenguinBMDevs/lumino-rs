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
    /// 对话框选择、格式校验与复制全部在后台线程完成（防 UI 冻结），
    /// 结果由 [`Root::poll_material_import`] 消费：成功后重新扫描列表。
    pub(crate) fn import_material_from_local(&mut self) {
        self.spawn_material_import();
    }
}
