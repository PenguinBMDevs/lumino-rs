//! `DialogManager` 事件转发实现 — 协作状态/光标与视频导出进度分发到对话框
//!
//! 自 `manager.rs` 拆分而来（零逻辑变更）。

use lumino_ui::state::root_state::DialogType;

use super::DialogManager;

impl DialogManager {
    /// 转发协作视图状态到所有已打开的协作对话框
    ///
    /// 协作唯一数据源是主窗口 Root（连接状态/邀请码/房间名由 runner 注入），
    /// 协作对话框为独立 Root，需在此同步最新视图状态，否则对话框永远停在
    /// “连接中”而无法进入房间。仅广播视图状态，**排除连接表单字段**，避免覆盖
    /// 用户正在输入的 host/port/username。
    pub fn forward_collaboration_view_state(
        &mut self,
        state: lumino_ui::state::root_state::CollaborationViewState,
        invite_code: Option<String>,
        room_name: Option<String>,
    ) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::Collaboration
                && let Some(ui) = dialog.ui_mut()
            {
                ui.set_collaboration_view_state(state, invite_code.clone(), room_name.clone());
                dialog.request_redraw();
            }
        }
    }

    /// 转发远端用户光标位置到所有已打开的协作对话框
    pub fn forward_collaboration_cursor(
        &mut self,
        user_id: String,
        x: f32,
        y: f32,
        color: String,
        username: String,
    ) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::Collaboration
                && let Some(ui) = dialog.ui_mut()
            {
                ui.update_remote_cursor(user_id.clone(), x, y, color.clone(), username.clone());
                dialog.request_redraw();
            }
        }
    }

    /// 转发远端用户离开事件到所有已打开的协作对话框
    pub fn forward_collaboration_user_left(&mut self, user_id: String) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::Collaboration
                && let Some(ui) = dialog.ui_mut()
            {
                ui.remove_remote_cursor(user_id.clone());
                dialog.request_redraw();
            }
        }
    }

    /// 转发视频导出进度到 VideoExport 对话框
    /// UI-015：把主窗口的音符总量推给工程设置对话框（值变化时才重绘）。
    pub fn forward_project_settings_note_count(&mut self, count: usize) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::ProjectSettings
                && let Some(ui) = dialog.ui_mut()
                && ui.set_project_settings_note_count(count)
            {
                dialog.request_redraw();
            }
        }
    }

    pub fn forward_video_export_progress(
        &mut self,
        message: String,
        progress: f64,
        total_frames: u64,
        render_fps: f64,
        elapsed_secs: f64,
    ) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::VideoExport
                && let Some(ui) = dialog.ui_mut()
            {
                ui.update_video_export_progress(
                    message.clone(),
                    progress,
                    total_frames,
                    render_fps,
                    elapsed_secs,
                );
                dialog.request_redraw();
            }
        }
    }

    /// 转发视频导出预览帧到 VideoExport 对话框
    pub fn forward_video_export_preview_frame(&mut self, data: Vec<u8>, w: u32, h: u32) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::VideoExport
                && let Some(ui) = dialog.ui_mut()
            {
                ui.update_video_export_preview_frame(data.clone(), w, h);
                dialog.request_redraw();
            }
        }
    }

    /// 转发视频导出完成到 VideoExport 对话框
    pub fn forward_video_export_completed(&mut self, elapsed_secs: f64) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::VideoExport
                && let Some(ui) = dialog.ui_mut()
            {
                ui.set_video_export_completed(elapsed_secs);
                dialog.request_redraw();
            }
        }
    }

    /// 转发视频导出失败到 VideoExport 对话框
    pub fn forward_video_export_failed(&mut self, error: String) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::VideoExport
                && let Some(ui) = dialog.ui_mut()
            {
                ui.set_video_export_failed(error.clone());
                dialog.request_redraw();
            }
        }
    }
}
