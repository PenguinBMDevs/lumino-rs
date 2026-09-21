//! Runner 文件菜单：保存完成 / 失败提示

use crate::runner::RunnerInner;

impl RunnerInner {
    /// 保存完成：记录路径 + 底边栏提示（3 秒）+ 云端自动回传
    pub(super) fn handle_save_completed(&mut self, path: std::path::PathBuf) {
        // 记录保存路径（后续 Ctrl+S 直接覆盖保存原文件）
        self.midi_state.current_midi_source = Some(path.clone());
        tracing::info!("工程保存完成，路径已记录：{:?}", path);

        // 底边栏显示"文件已经保存"，3 秒后自动恢复"就绪"
        let language = self.window_state.window.ui().settings().display.language;
        let saved_msg = lumino_extras::i18n::main_translations(language)
            .status_file_saved
            .to_string();
        self.window_state
            .window
            .ui_mut()
            .set_status_message(Some(saved_msg));
        self.spawn_status_hint_timeout();

        // 从云端打开的文件：自动上传回云端原路径
        // （若已有上传进行中，run_cloud_upload_overwrite 会直接拒绝本次回传，
        //   不排队不补传——用户可待上传完成后再次 Ctrl+S）
        if let Some(src) = self.midi_state.cloud_source.clone() {
            tracing::info!("工程来自云端，自动上传回原路径：{}", src.remote_path);
            self.run_cloud_upload_overwrite(src.conn_id, src.remote_path, path);
        }

        // 保存确认对话框选择「保存」后，保存完成即继续挂起的关闭动作
        // （关闭工程 / 打开另一个工程 / 退出）。无挂起动作时为空操作。
        if self.run_pending_after_save {
            self.finish_pending_close_action();
        }

        // 保存完成后工程处于干净状态（无未保存更改）
        self.window_state.window.ui_mut().mark_project_clean();
    }

    /// 保存失败：底边栏提示错误原因（3 秒后自动恢复）
    pub(super) fn handle_save_failed(&mut self, msg: String) {
        tracing::error!("保存失败：{msg}");
        // 保存确认对话框选择「保存」但保存失败：放弃挂起的关闭动作，
        // 避免卡在半关闭状态或强制关闭未保存的工程。
        if self.run_pending_after_save {
            tracing::warn!("保存确认：保存失败，放弃挂起的关闭操作");
            self.pending_close_action = None;
            self.run_pending_after_save = false;
        }
        let language = self.window_state.window.ui().settings().display.language;
        let fail_msg = format!(
            "{}：{msg}",
            lumino_extras::i18n::main_translations(language).status_save_failed
        );
        self.window_state
            .window
            .ui_mut()
            .set_status_message(Some(fail_msg));
        self.spawn_status_hint_timeout();
    }

    /// 3 秒后发送提示超时事件，清除底边栏状态消息
    fn spawn_status_hint_timeout(&self) {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                lumino_ui::event::menu::file::Event::save_hint_timeout(),
            ));
        });
    }
}
