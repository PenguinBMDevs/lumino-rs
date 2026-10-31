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
            DialogResult::CustomPrecision(spec) => {
                tracing::info!("应用自定义吸附: {:?}", spec);
                ui.set_custom_snap(spec);
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
            DialogResult::InsertShapeDrawing { drawing, options } => {
                tracing::info!("形状抽屉：插入工程 {}", drawing.name);
                if let Err(error) = ui.insert_shape_drawing(&drawing, options) {
                    tracing::warn!("形状抽屉插入失败: {error}");
                }
            }
            DialogResult::HistoryJump(target) => {
                tracing::info!("撤销历史：跳到游标 {target}");
                if !ui.history_jump_to(target) {
                    tracing::warn!("撤销历史跳转被拒绝（可能正在编辑 / 有待确认笔画）");
                }
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

/// 启动时把 `ui_state` 的几何字段一次性迁移为**逻辑像素**（REF-001 阶段 G-2）
///
/// # 为什么必须迁移
///
/// 历史版本写入侧用 `PhysicalSize`/`PhysicalPosition`，读取侧
/// （`WindowManager::build_window_attributes`）用 `LogicalSize`/`LogicalPosition`：
/// 150% 缩放下逻辑 1440×900 = 物理 2160×1350，存进去 2160×1350、下次按逻辑 2160×1350
/// 建窗 ⇒ 物理 3240×2025 ⇒ **每次重启 ×1.5**，位置同比例漂移。
///
/// # 三个必须守住的行为
///
/// 1. **只迁一次**：`UiState::version` 达标即返回，绝不重复除 `scale`；
/// 2. **立刻落盘**：`version` 只活在内存里的话，下次启动会再除一次 ⇒ 窗口越迁越小；
/// 3. **夹到工作区**：历史累积的异常大值（如 3240 宽）不能原样还原，否则用户升级后
///    第一眼就是超出屏幕的巨窗。
pub(crate) fn migrate_ui_state_units(
    storage: &mut crate::storage::Storage,
    event_loop: &winit::event_loop::ActiveEventLoop,
) {
    use super::super::window_manager::geometry;

    if !geometry::needs_migration(storage.ui_state.get()) {
        return;
    }

    let (scale, work_area) = geometry::primary_monitor_metrics(event_loop);
    storage.ui_state.patch(|state| {
        // 返回值只用于单测断言；这里唯一目的是把 `state` 就地改写
        let _migrated = geometry::migrate_units(state, scale, work_area);
    });

    if let Err(e) = storage.ui_state.save() {
        tracing::warn!("ui_state 单位迁移落盘失败（下次启动将重试）: {e}");
    }

    tracing::info!("ui_state 尺寸单位已迁移为逻辑像素（scale={scale:.2}，工作区={work_area:?}）");
}
