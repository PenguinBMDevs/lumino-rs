//! 关闭与取消类对话框事件：标记关闭、清理 pending 与取消标志。

use std::sync::atomic::Ordering;

use crate::runner::{RunnerInner, dialog_manager::DialogType};

impl RunnerInner {
    pub(super) fn on_close_draw_settings(&mut self) {
        self.close_dialog_traced(DialogType::DrawSettings, "音符画设置");
    }

    pub(super) fn on_close_shape_drawer(&mut self) {
        self.close_dialog_traced(DialogType::ShapeDrawer, "形状抽屉");
    }

    pub(super) fn on_close_history(&mut self) {
        self.close_dialog_traced(DialogType::History, "撤销历史");
    }

    pub(super) fn on_close_help(&mut self) {
        self.close_dialog_traced(DialogType::Help, "帮助");
    }

    pub(super) fn on_close_custom_precision(&mut self) {
        self.close_dialog_traced(DialogType::CustomPrecision, "自定义精度");
    }

    pub(super) fn on_close_collaboration(&mut self) {
        self.close_dialog_traced(DialogType::Collaboration, "协作");
    }

    pub(super) fn on_close_project_settings(&mut self) {
        self.close_dialog_traced(DialogType::ProjectSettings, "工程设置");
    }

    pub(super) fn on_close_memory_monitor(&mut self) {
        self.close_dialog_traced(DialogType::MemoryMonitor, "内存监控");
    }

    pub(super) fn on_close_video_export(&mut self) {
        self.window_state
            .dialog_manager
            .mark_dialog_for_close(DialogType::VideoExport);
        // 设置取消标志，让后台 video-render 线程退出
        self.window_state
            .video_export_cancel
            .store(true, Ordering::Relaxed);
    }

    pub(super) fn on_close_speed_change(&mut self) {
        self.close_dialog_traced(DialogType::SpeedChange, "音符变速");
    }

    pub(super) fn on_close_batch_edit(&mut self) {
        self.close_dialog_traced(DialogType::BatchEdit, "批量编辑");
    }

    pub(super) fn on_close_recover_track(&mut self) {
        self.close_dialog_traced(DialogType::RecoverTrack, "找回删除音轨");
        // 清空 pending（防止对话框已关闭后 pending 残留）
        self.pending_recover_track_entries = None;
    }

    pub(super) fn on_abort_audio_export(&mut self) {
        if let Some(ctrl) = &self.window_state.audio_export_control {
            ctrl.abort();
            tracing::info!("音频导出已请求中止");
        } else {
            tracing::warn!("中止音频导出：无进行中的导出任务");
        }
        // 立即清理控制句柄，避免暂停状态残留
        // 实际文件删除由渲染线程的 abort 分支处理
    }
}
