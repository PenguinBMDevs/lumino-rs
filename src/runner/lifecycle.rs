//! Runner 生命周期管理模块
//!
//! 此模块已拆分为多个子模块：
//! - dialog: 对话框事件处理
//! - memory: 内存日志功能
//! - midi: MIDI 重初始化
//! - control_flow: 事件循环控制流
//! - test_mode: 测试模式 FPS 监测

mod control_flow;
mod dialog;
mod memory;
mod midi;
mod test_mode;

use super::inner::{Runner, TestModeState};

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.inner.is_some() || self.init_error.is_some() {
            return;
        }

        match self.init_inner(event_loop) {
            Ok(inner) => {
                self.inner = Some(inner);

                // 如果是测试模式，自动加载 MIDI
                if let Some(test_config) = self.test_config.take()
                    && let Some(this) = self.inner.as_mut()
                {
                    tracing::info!("测试模式：准备加载 MIDI - {}", test_config.midi_path);
                    let midi_path = std::path::PathBuf::from(&test_config.midi_path);
                    let progress_cb = this.window_state.progress_cb.clone();
                    let test_duration = test_config.test_time;

                    this.window_state.window.ui_mut().skip_ui_rendering = true;
                    this.test_state.test_mode_state = Some(TestModeState {
                        active: false,
                        start_time: None,
                        duration: test_duration,
                        fps_samples: Vec::new(),
                        last_fps_update: None,
                        frame_count: 0,
                    });

                    tokio::spawn(async move {
                        match lumino_core::midi::loader::load_parsed_midi(
                            midi_path,
                            Some(&progress_cb),
                        )
                        .await
                        {
                            Ok(parsed) => {
                                tracing::info!("测试模式：MIDI 加载完成");
                                lumino_core::event::emit(lumino_core::event::Event::Menu(
                                    lumino_core::event::menu::Event::File(
                                        lumino_core::event::menu::file::Event::MidiParsed(
                                            std::sync::Arc::new(parsed),
                                        ),
                                    ),
                                ));
                            }
                            Err(e) => {
                                tracing::error!("测试模式：MIDI 加载失败 - {e}");
                                lumino_core::event::emit(lumino_core::event::Event::Menu(
                                    lumino_core::event::menu::Event::File(
                                        lumino_core::event::menu::file::Event::MidiParseError(
                                            e.to_string(),
                                        ),
                                    ),
                                ));
                            }
                        }
                    });
                }
            }
            Err(e) => {
                tracing::error!("Runner 初始化失败：{}", e);
                self.init_error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        puffin::profile_function!();
        puffin::profile_scope!("runner_window_event");

        let Some(this) = self.inner.as_mut() else {
            return;
        };

        // 首先检查是否是进度窗口
        if this.window_state.progress.is_progress_window(window_id) {
            this.window_state.progress.handle_event(event);
            return;
        }

        // 检查是否是对话框窗口
        if this.handle_dialog_event(event_loop, window_id, event.clone()) {
            return;
        }

        // 主窗口事件
        this.window_state
            .window
            .handle_event(event, &mut this.window_state.storage);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        puffin::profile_scope!("runner_about_to_wait");

        let Some(this) = self.inner.as_mut() else {
            return;
        };

        // 处理进度消息
        puffin::profile_scope!("runner_about_to_wait_process_messages");
        let main_window = this.window_state.window.window().clone();
        let main_ui = this.window_state.window.ui_mut();
        this.window_state
            .progress
            .process_messages(main_ui, &main_window);

        // 更新进度窗口
        puffin::profile_scope!("runner_about_to_wait_progress_update");
        let ui_config = this.window_state.storage.config.get().ui.clone();
        this.window_state.progress.update(event_loop, &ui_config);

        // 处理窗口动作
        puffin::profile_scope!("runner_about_to_wait_window_actions");
        this.window_state.window.handle_window_actions(event_loop);

        // 处理音频动作
        puffin::profile_scope!("runner_about_to_wait_audio_actions");
        crate::runner::inner::RunnerInner::process_audio_actions(
            &mut this.window_state.window,
            &mut this.midi_state.midi,
        );

        // 处理核心事件（包括打开对话框）
        puffin::profile_scope!("runner_about_to_wait_core_events");
        this.process_core_events(event_loop);

        // 初始化新创建的对话框（同步主窗口的协作状态）
        {
            puffin::profile_scope!("runner_about_to_wait_dialog_init");
            let main_ui = this.window_state.window.ui();
            this.window_state
                .dialog_manager
                .initialize_pending_with_collaboration_state(
                    event_loop,
                    this.window_state.window.window(),
                    &this.window_state.storage.config.get().ui,
                    main_ui,
                );
        }

        // 更新对话框
        puffin::profile_scope!("runner_about_to_wait_dialog_update");
        this.window_state.dialog_manager.update();

        // 保存存储
        puffin::profile_scope!("runner_about_to_wait_save_storage");
        this.save_storage();

        // 内存日志
        puffin::profile_scope!("runner_about_to_wait_memory_logging");
        this.handle_memory_logging();

        // 重新初始化 MIDI 或检查 XSynth 异步初始化
        puffin::profile_scope!("runner_about_to_wait_midi_reinit");
        this.handle_midi_reinit();

        // 检查是否需要重启窗口（标题栏设置变更）
        if this.window_state.needs_window_restart {
            this.window_state.needs_window_restart = false;
            this.restart_window(event_loop);
        }

        // 控制循环休眠策略
        puffin::profile_scope!("runner_about_to_wait_control_flow");
        this.handle_control_flow(event_loop);

        // 测试模式 FPS 监测
        puffin::profile_scope!("runner_about_to_wait_test_mode_fps");
        this.handle_test_mode_fps(event_loop);
    }
}
