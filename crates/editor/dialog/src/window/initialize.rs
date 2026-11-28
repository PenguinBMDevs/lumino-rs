//! 对话框窗口 - GFX 与 UI 初始化

use std::sync::Arc;

use lumino_core::{storage::config::UiConfig, tr};
use lumino_ui::state::root_state::DialogType;

use crate::manager::PendingDialog;

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
            {
                let window = Arc::clone(&self.window);
                move |instance| instance.create_surface(Arc::clone(&window))
            },
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
        pending: &PendingDialog,
    ) -> Result<(), String> {
        puffin::profile_scope!("dialog_init_phase_ui");
        let physical_size = self.window.inner_size();

        if physical_size.width == 0 || physical_size.height == 0 {
            return Err("窗口大小为零，无法初始化".to_string());
        }

        let gfx = self.gfx.as_ref().ok_or("GFX 未初始化")?;

        if let Some(title) = pending.pending_title.as_deref() {
            self.window.set_title(title);
        }

        // 窗口按 trait 对象注入：`Host` 不再依赖具体 winit 窗口类型。
        let host_window = lumino_ui::host::host_window(self.window.clone());
        let mut ui = match self.dialog_type {
            DialogType::Settings => lumino_ui::Host::new_settings_dialog(
                host_window.clone(),
                physical_size.width,
                physical_size.height,
                ui_config,
                gfx,
            ),
            _ => lumino_ui::Host::new_dialog(
                host_window,
                physical_size.width,
                physical_size.height,
                ui_config,
                gfx,
                self.dialog_type,
            ),
        };

        // 初始状态注入拆成三处分派：各自覆盖互不重叠的 `DialogType` 子集，
        // 每个类型仍然只命中一处，与原来单次 `match` 的执行结果逐一对应。
        seed_document_state(&mut ui, self.dialog_type, main_ui);
        seed_pending_state(&mut ui, self.dialog_type, pending, main_ui);
        set_dialog_open_flags(&mut ui, self.dialog_type);

        // REF-001：**不在此处显示窗口**。窗口要等「入册后的首帧」测量完内容尺寸
        // 再显示（`window/size.rs` 的 `apply_measured_size`，由
        // `DialogManager::update` 调用）——那时同帧的内容注入也已完成。
        self.ui = Some(ui);

        Ok(())
    }
}

/// 用主窗口快照注入对话框初始状态
///
/// 对话框 Root 是独立的，拿不到主窗口数据，因此文档类状态必须显式拉取。
fn seed_document_state(
    ui: &mut lumino_ui::Host,
    dialog_type: DialogType,
    main_ui: &lumino_ui::Host,
) {
    match dialog_type {
        DialogType::Collaboration => {
            ui.sync_collaboration_state_from(main_ui);
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
                use10,
            ) = main_ui.get_project_settings_data();
            ui.set_project_settings_data(lumino_ui::root::ProjectSettingsDialogData {
                title,
                tempo,
                copyright,
                author,
                created_display,
                total_editing_time_seconds: editing_time,
                note_count: main_ui.get_editor_note_count(),
                note_count_display: main_ui.note_count_display(),
                time_signatures,
                use10,
                port_soundfonts: main_ui.get_project_port_soundfonts(),
            });
        }
        DialogType::PortSoundfonts => {
            // REND-005：映射与文档端口数都来自主窗口；对话框只编辑草稿。
            ui.set_port_soundfonts_dialog(
                main_ui.get_project_port_soundfonts(),
                main_ui.document_max_port(),
            );
        }
        DialogType::Settings => {
            // MIDI 输入设备页需要主窗口的输入 API 与设备快照（设置面板为独立 Root）；
            // 必须在 `set_settings_dialog_open` 之前注入，后者会立刻发起后台扫描。
            ui.sync_midi_state_from(main_ui);
            ui.set_settings_dialog_open(true);
            // 云管理页需要主窗口的连接快照（设置面板为独立 Root）
            ui.sync_cloud_state_from(main_ui);
            // 洋葱皮后端锁定态的唯一真源是主窗口（是否有 MIDI 文档）：
            // 对话框 Root 里没有文档，必须显式注入，否则设置页会一直可切换。
            ui.set_settings_onion_backend_locked(main_ui.onion_backend_locked());
        }
        DialogType::CloudConnect | DialogType::CloudBrowser | DialogType::CloudNotice => {
            // 同步主窗口的云存储快照（连接列表/表单回显/提醒内容）。
            // 云存储唯一数据源是主窗口 Root，对话框为独立 Root 需拉取。
            ui.sync_cloud_state_from(main_ui);
        }
        DialogType::History => {
            ui.set_history_dialog_open(true);
            let (steps, current) = main_ui.history_view();
            ui.set_history_view(steps, current);
        }
        // 其余类型不读主窗口文档：由 `seed_pending_state` 或 `set_dialog_open_flags` 处理。
        _ => {}
    }
}

/// 用 Runner 注入的 `pending` 快照初始化对话框
///
/// 其中「画刷绘制行为」与「形状抽屉」还要主窗口的音轨候选与色号上限。
fn seed_pending_state(
    ui: &mut lumino_ui::Host,
    dialog_type: DialogType,
    pending: &PendingDialog,
    main_ui: &lumino_ui::Host,
) {
    match dialog_type {
        DialogType::LoadConfirm => {
            let path = pending.pending_path.as_deref().unwrap_or_default();
            ui.set_load_confirm_dialog(path, pending.pending_size_mb.unwrap_or(0.0));
        }
        DialogType::PortSoundfontsUnsupported => {
            let (backend, ports) = pending
                .pending_port_soundfonts_unsupported
                .clone()
                .unwrap_or_default();
            ui.set_port_soundfonts_unsupported_dialog(backend, ports);
        }
        DialogType::MaterialDelete => {
            // DRAW-045：下标与素材名快照由 Runner 注入（Dialog 层不读右侧栏）。
            let (index, name) = pending.pending_material_delete.clone().unwrap_or_default();
            ui.set_material_delete_dialog(index, name);
        }
        DialogType::TempoMaxBpm => {
            // 当前上限只用于预填；生效值由对话框结果回传。
            ui.set_tempo_max_bpm_dialog(pending.pending_tempo_max_bpm.unwrap_or(512.0));
        }
        DialogType::AutosaveRecover => {
            // DEBT-12：路径与修改时间由 Runner 注入（Dialog 层不碰文件系统）
            ui.set_autosave_recover_dialog(
                pending.pending_path.clone().unwrap_or_default(),
                pending.pending_modified_at.clone().unwrap_or_default(),
            );
        }
        DialogType::BrushSettings => {
            // 种入当前画刷配置作为对话框本地草稿，便于用户在此基础上编辑。
            if let Some(config) = pending.pending_brush_config.as_ref() {
                ui.set_brush_settings_draft(config.clone());
            }
            // 注入可选音轨列表（排除指挥轨），供「每层音轨」下拉选择。
            let tracks = main_ui.normal_track_choices();
            ui.set_brush_settings_tracks(tracks);
        }
        DialogType::ShapeDrawer => {
            if let Some(dir) = pending.shapes_dir.as_deref() {
                ui.set_shape_drawer_dir(dir.to_path_buf());
            }
            // DRAW-032：区域色号上限由主窗口注入（抽屉是独立 Root）
            ui.set_shape_drawer_use10(main_ui.get_project_use10());
            ui.set_shape_drawer_dialog_open(true);
        }
        // 其余类型不读 `pending`：由 `seed_document_state` 或 `set_dialog_open_flags` 处理。
        _ => {}
    }
}

/// 只切换对话框开关、不注入外部快照的类型
///
/// 本 `match` 显式列出全部 `DialogType`：新增变体时此处会编译报错，
/// 而不是被 `_` 静默吞掉。
fn set_dialog_open_flags(ui: &mut lumino_ui::Host, dialog_type: DialogType) {
    match dialog_type {
        DialogType::None => {}
        DialogType::CustomPrecision => {
            ui.set_custom_precision_dialog_open(true);
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
            ui.update_video_export_progress(tr!("正在初始化...").to_string(), 0.0, 0, 0.0, 0.0);
        }
        DialogType::MemoryMonitor => {
            ui.set_memory_monitor_dialog_open(true);
        }
        DialogType::RecoverTrack => {
            ui.set_recover_track_dialog_open(true);
        }
        DialogType::SaveConfirm => {
            ui.set_save_confirm_dialog();
        }
        DialogType::DrawSettings => {
            // 「音符画设置」当前为占位面板（设置项待接入），无草稿需要种入；
            // 但仍需切到本对话框类型，否则 overlay 会落到 `DialogType::None`
            // 的空容器分支（窗口一片空白）。
            ui.set_draw_settings_dialog_open(true);
        }
        DialogType::Help => {
            // 帮助窗口是独立 Root；切类型后 overlay 才会渲染帮助视图。
            ui.set_help_dialog_open(true);
        }
        DialogType::About => {
            // 关于窗口是独立 Root：`new_dialog` 已按 dialog_type 切到 About，
            // overlay 会直接渲染品牌行 + 版本号，无需额外注入状态。
        }
        DialogType::Mixer => {
            // 通道条快照由 Runner 在 `sync_mixer_dialog` 中逐帧注入；
            // 这里只切换对话框类型，让 overlay 渲染混音台视图。
            ui.set_mixer_dialog_open(true);
        }
        // 以下类型的初始状态由前两个助手注入，空臂只为让本 `match` 保持穷尽。
        DialogType::Collaboration
        | DialogType::LoadConfirm
        | DialogType::ProjectSettings
        | DialogType::PortSoundfonts
        | DialogType::PortSoundfontsUnsupported
        | DialogType::Settings
        | DialogType::MaterialDelete
        | DialogType::TempoMaxBpm
        | DialogType::AutosaveRecover
        | DialogType::CloudConnect
        | DialogType::CloudBrowser
        | DialogType::CloudNotice
        | DialogType::BrushSettings
        | DialogType::ShapeDrawer
        | DialogType::History => {}
    }
}
