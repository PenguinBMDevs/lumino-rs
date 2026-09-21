//! RunnerInner 生命周期辅助方法
//!
//! 自 `lifecycle.rs` 逐字节搬移的 `RunnerInner` inherent 实现（仅放宽可见性为 `pub(super)`）。

use std::sync::Arc;

use lumino_ui::state::root_state::DialogType;

impl crate::runner::inner::RunnerInner {
    /// 转发洋葱皮生成进度到进度窗口，并检测生成完成以设置编辑开始时间。
    pub(super) fn about_to_wait_waterfall_progress(&mut self) {
        puffin::profile_scope!("runner_about_to_wait_waterfall_progress");
        let waterfall_progress = self.window_state.window.ui().drain_waterfall_progress();
        if !waterfall_progress.is_empty() {
            let cb = Arc::clone(&self.window_state.progress_cb);
            for (msg, pct) in waterfall_progress {
                // 检测洋葱皮贴图生成完成（progress >= 1.0）
                if pct >= 1.0 && self.session_tracker.editing_start_time.is_none() {
                    self.session_tracker.editing_start_time = Some(std::time::Instant::now());
                    tracing::info!(
                        "洋葱皮贴图生成完成，编辑计时开始（累计 {} 秒）",
                        self.session_tracker.accumulated_editing_secs
                    );
                }
                cb(&msg, pct as f64);
            }
        }
    }

    /// 分帧初始化新创建的对话框，并同步主窗口的协作状态与主题。
    ///
    /// 每帧只推进一个对话框的一个初始化阶段（窗口 → GFX → UI），
    /// 避免在 `about_to_wait` 中单帧阻塞 900ms+。
    pub(super) fn about_to_wait_init_dialogs(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        puffin::profile_scope!("runner_about_to_wait_dialog_init");
        let main_ui = self.window_state.window.ui();
        // 从主窗口获取当前主题，覆盖 storage 中的主题缓存
        // 防止 save_storage 尚未持久化时对话框读取到过期主题
        let mut dialog_config = self.window_state.storage.config.get().ui.clone();
        dialog_config.theme = main_ui.root().theme().to_string();
        self.window_state
            .dialog_manager
            .process_initialization_step(
                event_loop,
                self.window_state.window.window(),
                &dialog_config,
                main_ui,
            );
    }

    /// 将视频导出线程产生的预览帧转发到 VideoExport 对话框窗口。
    pub(super) fn about_to_wait_forward_video_preview(&mut self) {
        puffin::profile_scope!("runner_about_to_wait_video_preview");
        if let Some(rx) = &mut self.window_state.video_preview_rx {
            while let Ok((data, w, h)) = rx.try_recv() {
                self.window_state
                    .dialog_manager
                    .forward_video_export_preview_frame(data, w, h);
            }
        }
    }

    /// 消费导出进度通道，按视频/音频分别转发到对话框或主窗口 UI。
    pub(super) fn about_to_wait_forward_export_progress(&mut self) {
        puffin::profile_scope!("runner_about_to_wait_export_progress");
        if let Some(rx) = &mut self.window_state.export_progress_rx {
            let main_ui = self.window_state.window.ui_mut();
            while let Ok((msg, progress, total_frames, render_fps, elapsed_secs)) = rx.try_recv() {
                // 判断是视频导出还是音频导出：
                // 检查是否存在 VideoExport 对话框窗口（导出在对话框中启动，
                // 主窗口的 overlay 不会变化）
                let is_video = self
                    .window_state
                    .dialog_manager
                    .has_dialog_type(DialogType::VideoExport);
                if is_video {
                    if progress < 0.0 {
                        self.window_state
                            .dialog_manager
                            .forward_video_export_failed(msg);
                        // 视频导出失败时关闭对话框
                        self.window_state
                            .dialog_manager
                            .mark_dialog_for_close(DialogType::VideoExport);
                    } else if progress >= 1.0 {
                        self.window_state
                            .dialog_manager
                            .forward_video_export_completed(elapsed_secs);
                    } else {
                        self.window_state
                            .dialog_manager
                            .forward_video_export_progress(
                                msg,
                                progress,
                                total_frames,
                                render_fps,
                                elapsed_secs,
                            );
                    }
                } else {
                    // 音频导出
                    if progress < 0.0 {
                        main_ui.update_export_progress(msg.clone(), 0.0);
                        main_ui.set_export_render_failed(msg);
                        // 清理控制句柄，避免暂停状态残留
                        self.window_state.audio_export_control = None;
                    } else {
                        main_ui.update_export_progress(msg, progress);
                        if progress >= 1.0 {
                            main_ui.set_export_render_completed();
                            self.window_state.audio_export_control = None;
                        }
                    }
                }
            }
        }
    }
}
