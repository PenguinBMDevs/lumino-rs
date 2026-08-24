//! 对话框窗口 - GFX 与 UI 初始化

use std::sync::Arc;

use lumino_core::storage::config::UiConfig;
use lumino_core::BrushConfig;
use lumino_ui::state::root_state::DialogType;

use super::DialogWindow;

impl DialogWindow {
    /// 获取 GFX 上下文的引用（用于判断当前处于哪个初始化阶段）
    pub(crate) fn gfx_ref(&self) -> Option<&lumino_gfx::Context> {
        self.gfx.as_ref()
    }

    /// 第二阶段：为已有窗口创建 GFX 上下文
    pub fn initialize_gfx(&mut self) -> Result<(), String> {
        puffin::profile_scope!("dialog_init_phase_gfx");
        let physical_size = self.window.inner_size();

        if physical_size.width == 0 || physical_size.height == 0 {
            return Err("窗口大小为零，无法初始化".to_string());
        }

        let gfx = lumino_gfx::Context::new_blocking(
            Arc::clone(&self.window),
            physical_size.width,
            physical_size.height,
        )
        .map_err(|e| format!("初始化图形上下文失败: {e}"))?;

        self.gfx = Some(gfx);
        Ok(())
    }

    /// 第三阶段：初始化 UI 并显示窗口
    ///
    /// 假设 GFX 已通过 `initialize_gfx` 初始化完成。本方法集中处理所有
    /// 对话框类型的 UI 创建与状态同步，避免为每种类型单独写一个初始化函数。
    pub fn initialize_ui(
        &mut self,
        ui_config: &UiConfig,
        main_ui: &lumino_ui::Host,
        pending_path: Option<&str>,
        size_mb: f64,
        pending_title: Option<&str>,
        pending_brush_config: Option<&BrushConfig>,
    ) -> Result<(), String> {
        puffin::profile_scope!("dialog_init_phase_ui");
        let physical_size = self.window.inner_size();

        if physical_size.width == 0 || physical_size.height == 0 {
            return Err("窗口大小为零，无法初始化".to_string());
        }

        let gfx = self.gfx.as_ref().ok_or("GFX 未初始化")?;

        if let Some(title) = pending_title {
            self.window.set_title(title);
        }

        let mut ui = match self.dialog_type {
            DialogType::Settings => lumino_ui::Host::new_settings_dialog(
                Arc::clone(&self.window),
                physical_size.width,
                physical_size.height,
                ui_config,
                gfx,
            ),
            _ => lumino_ui::Host::new_dialog(
                Arc::clone(&self.window),
                physical_size.width,
                physical_size.height,
                ui_config,
                gfx,
                self.dialog_type,
            ),
        };

        match self.dialog_type {
            DialogType::None => {}
            DialogType::CustomPrecision => {
                ui.set_custom_precision_dialog_open(true);
            }
            DialogType::Collaboration => {
                ui.sync_collaboration_state_from(main_ui);
            }
            DialogType::LoadConfirm => {
                let path = pending_path.unwrap_or_default();
                ui.set_load_confirm_dialog(path, size_mb);
            }
            DialogType::ProjectSettings => {
                ui.set_project_settings_dialog_open(true);
                let (
                    title,
                    tempo,
                    copyright,
                    author,
                    created_display,
                    editing_time,
                    time_signatures,
                ) = main_ui.get_project_settings_data();
                ui.set_project_settings_data(lumino_ui::root::ProjectSettingsDialogData {
                    title,
                    tempo,
                    copyright,
                    author,
                    created_display,
                    total_editing_time_seconds: editing_time,
                    time_signatures,
                });
            }
            DialogType::Settings => {
                ui.set_settings_dialog_open(true);
                // 云管理页需要主窗口的连接快照（设置面板为独立 Root）
                ui.sync_cloud_state_from(main_ui);
            }
            DialogType::SpeedChange => {
                ui.set_speed_change_dialog_open(true);
            }
            DialogType::BatchEdit => {
                ui.set_batch_edit_dialog_open(true);
            }
            DialogType::ExportProgress => {
                ui.set_export_progress_dialog_open(true);
            }
            DialogType::VideoExport => {
                ui.update_video_export_progress("正在初始化...".to_string(), 0.0, 0, 0.0, 0.0);
            }
            DialogType::MemoryMonitor => {
                ui.set_memory_monitor_dialog_open(true);
            }
            DialogType::RecoverTrack => {
                ui.set_recover_track_dialog_open(true);
            }
            DialogType::CloudConnect | DialogType::CloudBrowser | DialogType::CloudNotice => {
                // 同步主窗口的云存储快照（连接列表/表单回显/提醒内容）。
                // 云存储唯一数据源是主窗口 Root，对话框为独立 Root 需拉取。
                ui.sync_cloud_state_from(main_ui);
            }
            DialogType::BrushSettings => {
                // 种入当前画刷配置作为对话框本地草稿，便于用户在此基础上编辑。
                if let Some(config) = pending_brush_config {
                    ui.set_brush_settings_draft(config.clone());
                }
                // 注入可选音轨列表（排除指挥轨），供「每层音轨」下拉选择。
                let tracks = main_ui.normal_track_choices();
                ui.set_brush_settings_tracks(tracks);
            }
        }

        self.window.set_visible(true);
        self.ui = Some(ui);

        Ok(())
    }
}
