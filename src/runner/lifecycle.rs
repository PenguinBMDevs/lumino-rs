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

        // 处理导出进度消息（直接更新 audio_export_dialog 内嵌进度条）
        {
            puffin::profile_scope!("runner_about_to_wait_export_progress");
            if let Some(rx) = &mut this.window_state.export_progress_rx {
                let main_ui = this.window_state.window.ui_mut();
                while let Ok((msg, progress)) = rx.try_recv() {
                    if progress < 0.0 {
                        // 渲染失败（progress = -1.0）
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

        // 转发洋葱皮生成进度到进度窗口（渲染线程 → UI 线程 → ProgressManager）
        {
            puffin::profile_scope!("runner_about_to_wait_onion_progress");
            let onion_progress = this.window_state.window.ui().drain_onion_progress();
            if !onion_progress.is_empty() {
                let cb = this.window_state.progress_cb.clone();
                for (msg, pct) in onion_progress {
                    cb(&msg, pct as f64);
                }
            }
        }

        // 高精度贴图冷静期检查：到期后触发脏音轨重生成
        {
            puffin::profile_scope!("runner_about_to_wait_hires_regen");
            let dirty_tracks = this.window_state.window.ui_mut().check_hires_regen();
            if let Some(tracks) = dirty_tracks {
                // 收集重生成所需上下文（clone 出来避免循环里反复借用）
                let regen_context = {
                    let ui = this.window_state.window.ui();
                    let hash = ui.hires_midi_hash().map(|s| s.to_string());
                    let info = ui.hires_gen_info();
                    let ui_cfg = this.window_state.storage.config.get().ui.clone();
                    let config = lumino_gfx::HiResConfig {
                        enabled: ui_cfg.hires_onion_enabled,
                        measures_per_group: ui_cfg.hires_measures_per_group,
                        tile_width_px: ui_cfg.hires_tile_width_px,
                        cooldown_secs: ui_cfg.hires_cooldown_secs,
                        gpu_mem_limit_mb: ui_cfg.hires_gpu_mem_limit_mb,
                        group_tile_mem_limit_mb: 256,
                        cache_dir: lumino_gfx::HiResConfig::default().cache_dir,
                    };
                    hash.zip(info).map(|(h, i)| (h, i, config))
                };
                if let Some((midi_hash, (ppq, key_count, total_ticks), config)) = regen_context {
                    tracing::info!("高精度贴图冷静期到期，重生 {} 个脏音轨", tracks.len());
                    // 按音轨组分组，每个 group 只重生一次，避免同组重复生成
                    let mut tracks_by_group: std::collections::HashMap<u32, Vec<u16>> =
                        std::collections::HashMap::new();
                    for track_idx in &tracks {
                        let group = (*track_idx / lumino_gfx::TRACKS_PER_GROUP) as u32;
                        tracks_by_group.entry(group).or_default().push(*track_idx);
                    }

                    for (group, group_tracks) in &tracks_by_group {
                        let max_track = group_tracks.iter().copied().max().unwrap_or(0);
                        // 音轨总数取当前侧边栏音轨数与组内最大音轨索引+1 的较大值
                        let track_count = {
                            let ui = this.window_state.window.ui();
                            (ui.track_count() as u16).max(max_track + 1)
                        };

                        // 收集该 group 内所有音轨的最新音符
                        let group_start = (group * lumino_gfx::TRACKS_PER_GROUP as u32) as u16;
                        let group_end =
                            (group_start + lumino_gfx::TRACKS_PER_GROUP).min(track_count);
                        let mut group_notes =
                            Vec::with_capacity((group_end - group_start) as usize);
                        for t in group_start..group_end {
                            let notes = this.window_state.window.ui().get_track_notes_for_hires(t);
                            group_notes.push(notes);
                        }

                        let representative = group_tracks[0];
                        tracing::info!(
                            "高精度贴图冷静期到期重生: group={}, representative_track={}, group_tracks={}",
                            group,
                            representative,
                            group_notes.len()
                        );
                        this.window_state.window.ui_mut().send_hires_regen(
                            lumino_gfx::render_thread::HiResTrackParams::new(
                                representative,
                                group_notes,
                                ppq,
                                key_count,
                                total_ticks,
                                track_count,
                                config.clone(),
                                midi_hash.clone(),
                            ),
                        );
                    }
                }
            }
        }

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
            // 从主窗口获取当前主题，覆盖 storage 中的主题缓存
            // 防止 save_storage 尚未持久化时对话框读取到过期主题
            let mut dialog_config = this.window_state.storage.config.get().ui.clone();
            dialog_config.theme = main_ui.root().theme().to_string();
            this.window_state
                .dialog_manager
                .initialize_pending_with_collaboration_state(
                    event_loop,
                    this.window_state.window.window(),
                    &dialog_config,
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
