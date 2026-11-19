//! 文件菜单：「最近打开」列表的维护与发布。
//!
//! 真源是 `UiConfig.recent_files`（持久化）；UI 侧 `Root.recent_files` 只是
//! 菜单装配用的运行时快照。任何增删都必须走本模块，保证「落盘 + 推送 UI」成对发生。

use std::path::{Path, PathBuf};

use lumino_core::storage::config::{RECENT_FILES_LIMIT, RecentFile, forget, remember};

use crate::runner::RunnerInner;

impl RunnerInner {
    /// 记录一个最近文件：去重置顶、落盘、同步主窗口菜单快照。
    ///
    /// 调用点只看**用户成功打开/保存**的动作；导入、自动保存恢复、云下载、
    /// 命令行加载不得调用。
    pub(super) fn remember_recent_file(&mut self, path: PathBuf) {
        let now = crate::storage::now_unix_secs().unwrap_or(0);
        let mut files = self
            .window_state
            .storage
            .config
            .get()
            .ui
            .recent_files
            .clone();
        remember(&mut files, &path, now, RECENT_FILES_LIMIT);
        self.publish_recent_files(files);
    }

    /// 从最近文件列表移除指定路径（文件已不存在时自动清理）。
    pub(super) fn forget_recent_file(&mut self, path: &Path) {
        let mut files = self
            .window_state
            .storage
            .config
            .get()
            .ui
            .recent_files
            .clone();
        forget(&mut files, path);
        self.publish_recent_files(files);
    }

    /// 清空最近打开记录。
    pub(super) fn clear_recent_files(&mut self) {
        self.publish_recent_files(Vec::new());
    }

    /// 落盘并推送 UI 快照。
    ///
    /// 写盘失败只告警不回滚：内存中的列表与 UI 应保持一致，下次成功保存会再落盘。
    fn publish_recent_files(&mut self, files: Vec<RecentFile>) {
        self.window_state.storage.config.patch(|config| {
            config.ui.recent_files.clone_from(&files);
        });
        if let Err(e) = self.window_state.storage.config.save() {
            tracing::warn!("保存最近打开列表失败: {e}");
        }
        self.window_state
            .window
            .ui_mut()
            .root_mut()
            .set_recent_files(files);

        #[cfg(target_os = "macos")]
        self.refresh_native_menu();
    }

    /// macOS：原生 NSMenu 是静态构建的，最近列表变化后必须整表重建。
    #[cfg(target_os = "macos")]
    fn refresh_native_menu(&self) {
        let language = self.window_state.window.ui().settings().display.language;
        let recent = self
            .window_state
            .storage
            .config
            .get()
            .ui
            .recent_files
            .clone();
        if let Err(e) = crate::platform::macos::init(language, &recent) {
            tracing::warn!("重建 macOS 原生菜单失败: {e:?}");
        }
    }
}
