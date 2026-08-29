//! 窗口管理
//!
//! RunnerInner 中与窗口操作、对话框结果处理等相关的实现。

use std::sync::Arc;

use super::super::dialog_manager::DialogResult;
use super::super::midi_manager::{MidiManager, handle_audio_action};
use super::super::window_manager::WindowManager;
use super::*;

impl RunnerInner {
    /// 处理音频动作（播放/停止/音符发送）
    pub(crate) fn process_audio_actions(window: &mut WindowManager, midi: &mut MidiManager) {
        let actions = window.ui_mut().take_audio_actions();

        if !actions.is_empty() {
            tracing::debug!("Runner: 处理 {} 个音频动作", actions.len());
        }

        for action in actions {
            if let Some(output) = midi.output_mut() {
                handle_audio_action(output, action);
            }
        }
    }

    /// 将对话框结果应用到主窗口 UI
    pub(crate) fn apply_dialog_result_to_ui(ui: &mut lumino_ui::Host, result: DialogResult) {
        match result {
            DialogResult::CustomPrecision {
                numerator,
                denominator,
            } => {
                tracing::info!("应用自定义精度: {}/{}", numerator, denominator);

                // 应用到主窗口的编辑器
                if let (Ok(num), Ok(den)) = (numerator.parse::<f32>(), denominator.parse::<f32>()) {
                    // 从编辑器状态获取实际的 PPQ 值
                    let ppq = ui.ppq();
                    let ticks = Self::compute_custom_precision_ticks(ppq as f32, num, den);

                    ui.set_custom_precision(ticks);
                    tracing::info!("自定义精度已应用: {} ticks (PPQ={})", ticks, ppq);
                }
            }
            DialogResult::LoadConfirm => {
                // LoadConfirm 由 lifecycle.rs 处理，这里不应到达
                tracing::warn!("LoadConfirm 结果不应通过 apply_dialog_result_to_ui 处理");
            }
            DialogResult::ProjectSettings {
                title,
                tempo,
                copyright,
                author,
                time_signatures,
            } => {
                tracing::info!(
                    "应用工程设置: 标题={}, BPM={}, 版权={}, 作者={}, 拍号变化数={}",
                    title,
                    tempo,
                    copyright,
                    author,
                    time_signatures.len()
                );
                ui.apply_project_settings(title, tempo, copyright, author, time_signatures);
            }
            DialogResult::Settings { settings, theme } => {
                tracing::info!("应用设置面板配置，主题: {}", theme);
                ui.apply_settings(*settings, theme);
            }
            DialogResult::SpeedChange { factor } => {
                tracing::info!("应用音符变速: 倍率={}", factor);
                ui.apply_speed_change(factor);
            }
            DialogResult::BatchEdit {
                velocity,
                gate,
                key,
                tick,
            } => {
                tracing::info!("应用批量编辑");
                ui.apply_batch_edit(&velocity, &gate, &key, &tick);
            }
            DialogResult::BrushSettings(config) => {
                tracing::info!("应用画刷绘制行为配置: 粗细度={}", config.thickness);
                ui.apply_brush_settings(config);
            }
            DialogResult::Cancel => {
                tracing::debug!("取消操作，无需处理");
            }
            DialogResult::SaveConfirm(_) => {
                // SaveConfirm 由 lifecycle.rs 处理，这里不应到达
                tracing::warn!("SaveConfirm 结果不应通过 apply_dialog_result_to_ui 处理");
            }
            DialogResult::RecoverTrackRestore {
                path,
                original_index,
            } => {
                tracing::info!(
                    "找回删除音轨：恢复 path={:?} original_index={}",
                    path,
                    original_index
                );
                match lumino_project::load_deleted_track(&path) {
                    Ok((meta, data)) => {
                        let notes = data
                            .notes
                            .into_iter()
                            .map(|n| lumino_ui::event::window::track::TrackDeletionNote {
                                start_tick: n.start_tick,
                                end_tick: n.end_tick,
                                key: n.key,
                                velocity: n.velocity,
                                channel: n.channel,
                                port: n.port,
                            })
                            .collect();
                        let payload = lumino_ui::event::window::track::TrackDeletionPayload {
                            track_id: meta.track_id,
                            track_name: meta.track_name,
                            port: meta.port,
                            channel: meta.channel,
                            is_drum: meta.is_drum,
                            max_tick: meta.max_tick,
                            original_index,
                            notes,
                        };
                        ui.apply_track_restored(payload);
                    }
                    Err(e) => {
                        tracing::error!("找回删除音轨：加载缓存失败 path={:?} err={}", path, e);
                    }
                }
            }
            DialogResult::RecoverTrackPermanentlyDelete { path, track_id } => {
                tracing::info!(
                    "找回删除音轨：永久删除 path={:?} track_id={}",
                    path,
                    track_id
                );
                if let Err(e) = lumino_project::delete_permanently(&path) {
                    tracing::error!("找回删除音轨：永久删除缓存失败 path={:?} err={}", path, e);
                }
                // 无论磁盘删除是否成功，都释放 reserved track_id
                // （文件不存在时 delete_permanently 静默返回 Ok，此处总能到达）
                ui.apply_track_permanently_deleted(track_id);
            }
        }
    }

    /// 重启窗口（标题栏设置变更后）
    pub(crate) fn restart_window(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        tracing::info!("正在重启窗口以应用标题栏设置...");

        // 保存当前窗口状态
        let is_maximized = self.window_state.window.window().is_maximized();

        // 销毁当前窗口并创建新窗口
        let ui_state = self.window_state.storage.ui_state.get();
        let config = self.window_state.storage.config.get();

        // 创建新的窗口管理器（共享保存进行中标志，关闭拦截逻辑保持一致）
        match WindowManager::new(
            event_loop,
            ui_state,
            &config.ui,
            Arc::clone(&self.saving),
            Arc::clone(&self.cloud_saving),
        ) {
            Ok(new_window) => {
                // 替换窗口管理器
                self.window_state.window = new_window;

                // 恢复窗口最大化状态
                if is_maximized {
                    self.window_state.window.window().set_maximized(true);
                }

                tracing::info!("窗口重启完成");
            }
            Err(e) => {
                tracing::error!("重启窗口失败: {}", e);
            }
        }
    }
}
