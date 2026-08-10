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

use lumino_ui::state::root_state::DialogType;

use super::inner::{Runner, TestModeState};

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.inner.is_some() || self.init_error.is_some() {
            return;
        }

        match self.init_inner(event_loop) {
            Ok(inner) => {
                self.inner = Some(inner);

                // 兜底首帧：显式请求一次主窗口重绘。
                // Wait 模式下 about_to_wait 不再每轮强制 request_redraw，
                // 首帧后若无动画/播放，needs_redraw==false 即进入休眠，不会忙循环。
                if let Some(this) = self.inner.as_mut() {
                    this.window_state.window.request_redraw();
                }

                // 启动自动连接云存储（后台静默执行，失败不打扰用户）
                if let Some(this) = self.inner.as_mut() {
                    this.startup_auto_connect();
                }

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
                        match lumino_midi_loader::loader::load_parsed_midi(
                            midi_path,
                            Some(&progress_cb),
                        )
                        .await
                        {
                            Ok(parsed) => {
                                tracing::info!("测试模式：MIDI 加载完成");
                                lumino_ui::event::emit(lumino_ui::event::Event::Menu(
                                    lumino_ui::event::menu::Event::File(
                                        lumino_ui::event::menu::file::Event::MidiParsed(
                                            std::sync::Arc::new(parsed),
                                        ),
                                    ),
                                ));
                            }
                            Err(e) => {
                                tracing::error!("测试模式：MIDI 加载失败 - {e}");
                                lumino_ui::event::emit(lumino_ui::event::Event::Menu(
                                    lumino_ui::event::menu::Event::File(
                                        lumino_ui::event::menu::file::Event::MidiParseError(
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

        // 转发洋葱皮生成进度到进度窗口（渲染线程 → UI 线程 → ProgressManager）
        // 同时检测洋葱皮生成完成，设置编辑开始时间
        this.about_to_wait_onion_progress();

        // 注意：脏区域临时覆层只在 set_current_track 中发送，
        // 不在轮询周期发送——避免编辑当前音轨时立即生成覆层干扰编辑器渲染。
        // 覆层的目的：切换音轨后让旧音轨的编辑内容立即显示为洋葱皮，
        // 直到 force_hires_regen 后台重生完成并清理覆层。

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
        this.about_to_wait_init_dialogs(event_loop);

        // 找回删除音轨对话框 UI 就绪后，把 pending 条目列表注入对话框
        // 必须在 about_to_wait_init_dialogs 之后调用——此时对话框 UI 可能刚就绪
        this.try_fill_recover_track_entries();

        // 处理视频导出预览帧（转发到 VideoExport 对话框窗口）
        // 注意：必须在对话框初始化之后消费，否则导出线程在对话框创建前发送的
        // 预览帧/进度会被转发到一个不存在的对话框而丢失。
        this.about_to_wait_forward_video_preview();

        // 处理导出进度消息（转发到对应的对话框窗口）
        this.about_to_wait_forward_export_progress();

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

        // 保存完成后的延迟退出：保存期间用户请求关闭（close_pending），
        // 本地保存与云端上传均结束后自动退出
        if this.window_state.window.close_pending && !this.is_saving() && !this.is_cloud_saving() {
            tracing::info!("保存完成，执行延迟的退出请求");
            this.window_state.window.close_pending = false;
            event_loop.exit();
            return;
        }

        // 控制循环休眠策略
        puffin::profile_scope!("runner_about_to_wait_control_flow");
        this.handle_control_flow(event_loop);

        // 测试模式 FPS 监测
        puffin::profile_scope!("runner_about_to_wait_test_mode_fps");
        this.handle_test_mode_fps(event_loop);
    }
}

impl crate::runner::inner::RunnerInner {
    /// 转发洋葱皮生成进度到进度窗口，并检测生成完成以设置编辑开始时间。
    fn about_to_wait_onion_progress(&mut self) {
        puffin::profile_scope!("runner_about_to_wait_onion_progress");
        let onion_progress = self.window_state.window.ui().drain_onion_progress();
        if !onion_progress.is_empty() {
            let cb = self.window_state.progress_cb.clone();
            for (msg, pct) in onion_progress {
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
    fn about_to_wait_init_dialogs(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
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
    fn about_to_wait_forward_video_preview(&mut self) {
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
    fn about_to_wait_forward_export_progress(&mut self) {
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
                    } else {
                        main_ui.update_export_progress(msg, progress);
                        if progress >= 1.0 {
                            main_ui.set_export_render_completed();
                        }
                    }
                }
            }
        }
    }
}
