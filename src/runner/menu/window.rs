//! Runner 窗口事件处理

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use lumino_export::audio::config::{
    AudioChannelMode, AudioInterpolation, AudioRenderConfig, ProgressCallback, ThreadMode,
};
use lumino_ui::event::window::Event as WindowEvent;
use std::path::PathBuf;
use std::sync::Arc;

impl RunnerInner {
    /// 处理窗口事件
    pub(super) fn handle_window_event(&mut self, window_event: WindowEvent) {
        match window_event {
            WindowEvent::Dialog(e) => self.handle_dialog_events(e),
            WindowEvent::Collaboration(e) => self.handle_collaboration_events(e),
            WindowEvent::Sync(e) => self.handle_sync_events(e),
            _ => {}
        }
    }

    fn handle_dialog_events(&mut self, window_event: lumino_ui::event::window::dialog::Event) {
        use lumino_ui::event::window::dialog::Event::*;
        match window_event {
            OpenCustomPrecisionDialog => {
                tracing::info!("请求打开自定义精度对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::CustomPrecision);
            }
            CloseCustomPrecisionDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::CustomPrecision);
                tracing::info!("请求关闭自定义精度对话框");
            }
            ApplyCustomPrecision(_, _) => {
                // 应用精度（在对话框结果中处理）
            }
            OpenCollaborationDialog => {
                tracing::info!("请求打开协作对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::Collaboration);
            }
            CloseCollaborationDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("请求关闭协作对话框");
            }
            OpenProjectSettingsDialog => {
                tracing::info!("请求打开工程设置对话框");
                // 优先使用已保存的项目标题，回退到文件名
                let saved_title = self.window_state.window.ui().get_project_settings_title();
                let display_title = if saved_title.is_empty() {
                    self.midi_state
                        .current_midi_source
                        .as_ref()
                        .and_then(|p| p.file_stem())
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "无标题".to_string())
                } else {
                    saved_title
                };
                let title = format!("{} - Lumino Midi", display_title);
                self.window_state
                    .dialog_manager
                    .open_project_settings(title);
            }
            CloseProjectSettingsDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::ProjectSettings);
                tracing::info!("请求关闭工程设置对话框");
            }
            ApplyProjectSettings {
                title,
                tempo,
                copyright,
            } => {
                tracing::info!(
                    "应用工程设置: 标题={}, BPM={}, 版权={}",
                    title,
                    tempo,
                    copyright
                );
                // 应用设置到主窗口
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_project_settings(title, tempo, copyright);
            }
            OpenSpeedChangeDialog => {
                tracing::info!("请求打开音符变速对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::SpeedChange);
            }
            CloseSpeedChangeDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::SpeedChange);
                tracing::info!("请求关闭音符变速对话框");
            }
            ConfirmSpeedChange(factor) => {
                tracing::info!("应用音符变速: 倍率={}", factor);
                // 应用变速到主窗口
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_speed_change(factor);
            }
            OpenLoadConfirmDialog { .. } => {}
            StartAudioExport {
                midi_path,
                soundfont_path,
                output_path,
                sample_rate,
                channels,
                layer_limit,
                channel_threading,
                key_threading,
                interpolation,
                apply_limiter,
                disable_fade_out,
                linear_envelope,
            } => {
                use std::time::Instant;
                tracing::info!("开始音频导出: MIDI={midi_path}, SF2={soundfont_path}");

                let midi_path = PathBuf::from(&midi_path);
                let soundfont_path = PathBuf::from(&soundfont_path);
                let output_path = PathBuf::from(&output_path);

                // 解析枚举值（来自 UI 的 Debug 格式）
                let ch = match channels.as_str() {
                    "Mono" => AudioChannelMode::Mono,
                    _ => AudioChannelMode::Stereo,
                };
                let ct = match channel_threading.as_str() {
                    "None" => ThreadMode::None,
                    "Auto" => ThreadMode::Auto,
                    s if s.starts_with("Manual") => {
                        // format: "Manual(N)"
                        let n = s
                            .trim_start_matches("Manual(")
                            .trim_end_matches(')')
                            .parse::<u32>()
                            .unwrap_or(2);
                        ThreadMode::Manual(n)
                    }
                    _ => ThreadMode::Auto,
                };
                let kt = match key_threading.as_str() {
                    "None" => ThreadMode::None,
                    "Auto" => ThreadMode::Auto,
                    s if s.starts_with("Manual") => {
                        let n = s
                            .trim_start_matches("Manual(")
                            .trim_end_matches(')')
                            .parse::<u32>()
                            .unwrap_or(2);
                        ThreadMode::Manual(n)
                    }
                    _ => ThreadMode::Auto,
                };
                let ip = match interpolation.as_str() {
                    "None" => AudioInterpolation::Nearest,
                    _ => AudioInterpolation::Linear,
                };
                let layer_limit = if layer_limit == 0 {
                    None
                } else {
                    Some(layer_limit as usize)
                };

                // 打开导出进度对话框
                let progress_tx = self.window_state.dialog_manager.open_export_progress();

                // 创建进度回调
                let progress_callback: ProgressCallback =
                    Arc::new(move |msg: String, progress: f64| {
                        let _ = progress_tx.send((msg, progress));
                    });

                let config = AudioRenderConfig {
                    midi_path,
                    soundfonts: vec![soundfont_path],
                    output_path,
                    sample_rate,
                    channels: ch,
                    layer_limit,
                    channel_threading: ct,
                    key_threading: kt,
                    interpolation: ip,
                    apply_limiter,
                    disable_fade_out,
                    linear_envelope,
                    progress_callback: Some(progress_callback),
                };

                // 在后台线程执行渲染（避免阻塞 UI）
                std::thread::Builder::new()
                    .name("audio-render".into())
                    .spawn(move || {
                        let start = Instant::now();
                        match lumino_export::render_audio(&config) {
                            Ok(()) => {
                                let elapsed = start.elapsed();
                                tracing::info!(
                                    "音频导出完成: {:?}, 耗时 {:?}",
                                    config.output_path,
                                    elapsed
                                );
                            }
                            Err(e) => {
                                tracing::error!("音频导出失败: {e}");
                            }
                        }
                    })
                    .expect("无法创建音频渲染线程");
            }
        }
    }

    fn handle_collaboration_events(
        &mut self,
        window_event: lumino_ui::event::window::collaboration::Event,
    ) {
        use lumino_ui::event::window::collaboration::Event::*;
        match window_event {
            Connect {
                host,
                port,
                username,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 连接到 {}:{}, 用户名: {}, 邀请码: {:?}",
                    host,
                    port,
                    username,
                    invite_code
                );
                let room_name = Some("Lumino 房间".to_string());
                self.handle_collaboration_connect(host, port, username, room_name, invite_code);
            }
            CreateRoom { name } => {
                self.handle_collaboration_create_room(name);
            }
            JoinRoom { invite_code } => {
                self.handle_collaboration_join_room(invite_code);
            }
            Disconnect => {
                self.handle_collaboration_disconnect();
            }
            Authenticated {
                user_id,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 认证成功事件 - 用户ID: {}, 目前默认邀请码: {}",
                    user_id,
                    invite_code
                );

                self.window_state
                    .window
                    .ui_mut()
                    .root_mut()
                    .state_mut()
                    .collaboration_dialog
                    .connection_status
                    .clear();

                if let Some(target_invite_code) = self.collab_state.pending_invite_code.take() {
                    tracing::info!("使用首屏填写的邀请码直接加入房间: {}", target_invite_code);
                    self.handle_collaboration_join_room(target_invite_code);
                } else {
                    self.window_state
                        .window
                        .ui_mut()
                        .set_collaboration_view_state(
                            lumino_ui::CollaborationViewState::RoomActions,
                            Some(invite_code),
                            None,
                        );
                }
            }
            RoomCreated {
                room_name,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 房间创建成功 - 房间名: {}, 邀请码: {}",
                    room_name,
                    invite_code
                );
                self.window_state
                    .window
                    .ui_mut()
                    .set_collaboration_view_state(
                        lumino_ui::CollaborationViewState::InRoom,
                        Some(invite_code),
                        Some(room_name),
                    );
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("协作: 自动关闭协作对话框");
            }
            RoomJoined {
                room_name,
                invite_code,
                user_count,
            } => {
                tracing::info!(
                    "协作: 加入房间成功 - 房间名: {}, 邀请码: {}, 用户数: {}",
                    room_name,
                    invite_code,
                    user_count
                );
                self.window_state
                    .window
                    .ui_mut()
                    .set_collaboration_view_state(
                        lumino_ui::CollaborationViewState::InRoom,
                        Some(invite_code),
                        Some(room_name),
                    );
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("协作: 自动关闭协作对话框");
            }
            Disconnected => {
                tracing::info!("协作: 连接断开事件");
                self.window_state
                    .window
                    .ui_mut()
                    .set_collaboration_view_state(
                        lumino_ui::CollaborationViewState::Connect,
                        None,
                        None,
                    );
            }
            MouseUpdate {
                user_id,
                x,
                y,
                color,
                username,
            } => {
                tracing::debug!(
                    "窗口事件 - 远程鼠标更新：user_id={}, x={}, y={}, color={}, username={}",
                    user_id,
                    x,
                    y,
                    color,
                    username
                );
                self.window_state
                    .window
                    .ui_mut()
                    .update_remote_cursor(user_id, x, y, color, username);
                self.window_state.window.window().request_redraw();
            }
            NoteUpdate { user_id, operation } => {
                self.handle_remote_note_update(user_id, operation);
                self.window_state.window.window().request_redraw();
            }
            ProjectUpdate { user_id, update } => {
                self.handle_remote_project_update(user_id, update);
                self.window_state.window.window().request_redraw();
            }
            UserLeft { user_id } => {
                self.window_state
                    .window
                    .ui_mut()
                    .remove_remote_cursor(user_id);
                self.window_state.window.window().request_redraw();
            }
        }
    }

    fn handle_sync_events(&mut self, window_event: lumino_ui::event::window::sync::Event) {
        use lumino_ui::event::window::sync::Event::*;
        match window_event {
            LocalNoteAdded {
                tick,
                key,
                length,
                velocity,
                channel,
                track_index,
            } => {
                self.handle_local_note_added(tick, key, length, velocity, channel, track_index);
            }
            LocalNoteMoved {
                tick,
                key,
                length,
                tick_offset,
                key_offset,
                track_index,
            } => {
                self.handle_local_note_moved(
                    tick,
                    key,
                    length,
                    tick_offset,
                    key_offset,
                    track_index,
                );
            }
            LocalNoteDeleted {
                tick,
                key,
                length,
                velocity,
                channel,
                track_index,
            } => {
                self.handle_local_note_deleted(tick, key, length, velocity, channel, track_index);
            }
            LocalTrackAdded { track_index } => {
                self.handle_local_track_added(track_index);
            }
        }
    }
}
