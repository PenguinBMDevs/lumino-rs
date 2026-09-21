//! Runner 的 winit 应用生命周期事件处理
//!
//! 自 `lifecycle.rs` 逐字节搬移的 `ApplicationHandler` trait 实现。

use std::sync::Arc;

use lumino_ui::state::root_state::DialogType;

use crate::runner::inner::{InitError, Runner, TestModeState};
use crate::runner::lifecycle::device_gate::DeviceGate;

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.inner.is_some() || self.init_error.is_some() || self.device_warning.is_some() {
            return;
        }

        // 启动设备检查门控（测试模式 / LUMINO_SKIP_DEVICE_CHECK 时跳过）
        if self.test_config.is_none() && std::env::var_os("LUMINO_SKIP_DEVICE_CHECK").is_none() {
            match self.run_device_gate(event_loop) {
                DeviceGate::Proceed => {}
                DeviceGate::Waiting => return,
                DeviceGate::Quit => {
                    event_loop.exit();
                    return;
                }
                DeviceGate::StorageError(e) => {
                    tracing::error!("Runner 初始化失败：{}", e);
                    self.init_error = Some(InitError::Storage(e));
                    event_loop.exit();
                    return;
                }
            }
        }

        match self.init_inner(event_loop) {
            Ok(inner) => {
                self.inner = Some(inner);

                // 兜底首帧重绘 / 状态栏提示 / 云自动连接（与警告窗「继续」路径共用）
                self.after_inner_started();

                // 如果是测试模式，自动加载 MIDI
                if let Some(test_config) = self.test_config.take()
                    && let Some(this) = self.inner.as_mut()
                {
                    tracing::info!("测试模式：准备加载 MIDI - {}", test_config.midi_path);
                    let midi_path = std::path::PathBuf::from(&test_config.midi_path);
                    let progress_cb = Arc::clone(&this.window_state.progress_cb);
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

                    // 看门狗在加载 MIDI 前确保已启动，并标记加载状态：
                    // 测试模式同样受"只监控加载 MIDI 期间"约束
                    lumino_diagnostics::memory_monitor::watchdog::spawn_watchdog();
                    lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(true);

                    tokio::spawn(async move {
                        match lumino_midi_loader::loader::load_parsed_midi(
                            midi_path,
                            Some(&progress_cb),
                        )
                        .await
                        {
                            Ok(parsed) => {
                                tracing::info!("测试模式：MIDI 加载完成");
                                lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(false);
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
                                lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(false);
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

        // 设备检查警告窗优先分发（此时主窗口尚未初始化）
        if self
            .device_warning
            .as_ref()
            .is_some_and(|w| w.is_window(window_id))
        {
            if let Some(warning) = self.device_warning.as_mut() {
                warning.handle_event(event);
            }
            return;
        }

        let Some(this) = self.inner.as_mut() else {
            return;
        };

        // 首先检查是否是进度窗口
        if this.window_state.progress.is_progress_window(window_id) {
            this.window_state.progress.handle_event(event);
            return;
        }

        // 检查是否是云传输进度悬浮窗口
        if this
            .window_state
            .cloud_progress
            .is_cloud_progress_window(window_id)
        {
            this.window_state.cloud_progress.handle_event(event);
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

        // 设备检查警告窗阶段：主窗口未初始化，只处理警告窗的用户选择
        if self.inner.is_none() {
            self.about_to_wait_device_warning(event_loop);
            return;
        }

        let Some(this) = self.inner.as_mut() else {
            return;
        };

        // 处理进度消息
        puffin::profile_scope!("runner_about_to_wait_process_messages");
        let main_window = Arc::clone(this.window_state.window.window());
        let main_ui = this.window_state.window.ui_mut();
        this.window_state
            .progress
            .process_messages(main_ui, &main_window);

        // 转发洋葱皮生成进度到进度窗口（渲染线程 → UI 线程 → ProgressManager）
        // 同时检测洋葱皮生成完成，设置编辑开始时间
        this.about_to_wait_waterfall_progress();

        // 注意：脏区域临时覆层只在 set_current_track 中发送，
        // 不在轮询周期发送——避免编辑当前音轨时立即生成覆层干扰编辑器渲染。
        // 覆层的目的：切换音轨后让旧音轨的编辑内容立即显示为洋葱皮，
        // 直到 force_waterfall_regen 后台重生完成并清理覆层。

        // 更新进度窗口
        puffin::profile_scope!("runner_about_to_wait_progress_update");
        let ui_config = this.window_state.storage.config.get().ui.clone();
        this.window_state.progress.update(event_loop, &ui_config);

        // 云传输进度悬浮窗：覆盖定位在云浏览对话框（无则主窗口）上
        puffin::profile_scope!("runner_about_to_wait_cloud_progress_update");
        this.window_state.cloud_progress.process_messages();
        let cloud_anchor: Option<&winit::window::Window> = match this
            .window_state
            .dialog_manager
            .dialog_window_of_type(DialogType::CloudBrowser)
        {
            Some(w) => Some(w.as_ref()),
            None => Some(this.window_state.window.window().as_ref()),
        };
        this.window_state
            .cloud_progress
            .update(event_loop, &ui_config, cloud_anchor);

        // 处理窗口动作
        puffin::profile_scope!("runner_about_to_wait_window_actions");
        this.window_state.window.handle_window_actions(event_loop);

        // 窗口关闭被保存确认对话框挂起（工程存在未保存更改）：弹出确认对话框
        // 交由 Runner 决定「保存 / 关闭（放弃）/ 取消」。读取并清空该标志。
        if this.window_state.window.take_deferred_save_confirm_close() {
            use crate::runner::inner::PendingCloseAction;
            this.request_close_action(PendingCloseAction::WindowClose, event_loop);
        }

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

        // GPU 兼容性检查结果注入（设置对话框可能刚创建/刚就绪）
        this.inject_pending_gpu_check_ui();

        // 处理视频导出预览帧（转发到 VideoExport 对话框窗口）
        // 注意：必须在对话框初始化之后消费，否则导出线程在对话框创建前发送的
        // 预览帧/进度会被转发到一个不存在的对话框而丢失。
        this.about_to_wait_forward_video_preview();

        // 处理导出进度消息（转发到对应的对话框窗口）
        this.about_to_wait_forward_export_progress();

        // 更新对话框
        puffin::profile_scope!("runner_about_to_wait_dialog_update");
        this.window_state.dialog_manager.update();

        // 设置面板切换主题：立即全局应用（主窗口 + 所有对话框），
        // 与 View 菜单复用 apply_global_theme；配置持久化交给随后同一帧的
        // save_storage。消费后设置对话框主题已是新值，回灌不会再次入队。
        if let Some(theme) = this
            .window_state
            .dialog_manager
            .take_pending_settings_theme()
        {
            tracing::info!("设置面板切换主题，立即全局应用: {}", theme);
            this.apply_global_theme(theme);
        }

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
