use std::sync::Arc;
use winit::event_loop::ControlFlow;

use super::dialog_manager::DialogResult;
use super::inner::{InitError, Runner, RunnerInner};

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.inner.is_some() || self.init_error.is_some() {
            return;
        }

        match self.init_inner(event_loop) {
            Ok(inner) => {
                self.inner = Some(inner);
            }
            Err(e) => {
                tracing::error!("Runner 初始化失败: {}", e);
                self.init_error = Some(e);
                // 退出事件循环，让 main 函数处理错误
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };

        // 首先检查是否是进度窗口
        if this.progress.is_progress_window(window_id) {
            this.progress.handle_event(event);
            return;
        }

        // 检查是否是对话框窗口
        if this.dialog_manager.is_dialog_window(window_id) {
            let mut dialog_result = None;
            let mut should_close = false;

            if let Some(dialog) = this.dialog_manager.get_dialog_mut(window_id) {
                dialog.handle_event(event);

                // 检查对话框是否应该关闭
                should_close = dialog.should_close();

                // 检查对话框结果
                if let Some(result) = dialog.check_result() {
                    dialog_result = Some(result);
                    should_close = true;
                }
            }

            // 如果应该关闭，关闭对话框
            if should_close {
                this.dialog_manager.close_dialog(window_id);
            } else {
                // 请求重绘对话框
                if let Some(dialog) = this.dialog_manager.get_dialog_mut(window_id) {
                    dialog.redraw();
                }
            }

            // 处理对话框返回的结果
            if let Some(result) = dialog_result {
                let main_ui = this.window.ui_mut();
                Runner::apply_dialog_result_to_ui(main_ui, result);
            }
            return;
        }

        // 主窗口事件
        this.window.handle_event(event, &mut this.storage);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };

        // 处理进度消息
        let main_window = this.window.window().clone();
        let main_ui = this.window.ui_mut();
        this.progress.process_messages(main_ui, &main_window);

        // 更新进度窗口
        let ui_config = this.storage.config.get().ui.clone();
        this.progress.update(event_loop, &ui_config);

        // 处理窗口动作
        this.window.handle_window_actions(event_loop);

        // 处理音频动作
        Runner::process_audio_actions(&mut this.window, &mut this.midi);

        // 处理核心事件（包括打开对话框）
        this.process_core_events(event_loop);

        // 初始化新创建的对话框（同步主窗口的协作状态）
        {
            let main_ui = this.window.ui();
            this.dialog_manager
                .initialize_pending_with_collaboration_state(
                    event_loop,
                    this.window.window(),
                    &this.storage.config.get().ui,
                    main_ui,
                );
        }

        // 更新对话框
        this.dialog_manager.update();

        // 保存存储
        this.save_storage();

        // 重新初始化 MIDI 如果需要
        if this.midi.needs_reinit() {
            let ui_config = this.storage.config.get().ui.clone();
            this.midi.reinit_if_needed(&ui_config);
        }

        // 检查 XSynth 异步初始化是否完成
        this.midi.check_async_init_complete();

        // 检查是否需要重启窗口（标题栏设置变更）
        if this.needs_window_restart {
            this.needs_window_restart = false;
            this.restart_window(event_loop);
        }

        // 检查播放状态：播放时使用 Poll 模式确保持续重绘，暂停时使用 Wait 模式节省资源
        let is_playing = this.window.ui().is_playing();
        if is_playing {
            event_loop.set_control_flow(ControlFlow::Poll);
            this.window.request_redraw();
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}
