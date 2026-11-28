//! 打开类对话框事件：创建窗口、去重聚焦与前置数据准备。

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use lumino_core::{BrushConfig, tr};

impl RunnerInner {
    pub(super) fn on_open_custom_precision(&mut self) {
        self.open_dialog_traced(DialogType::CustomPrecision, "自定义精度");
    }

    pub(super) fn on_open_brush_settings(&mut self, config: BrushConfig) {
        self.window_state.dialog_manager.open_brush_settings(config);
    }

    pub(super) fn on_open_draw_settings(&mut self) {
        // 齿轮是"开合"语义，但独立窗口不能靠再点一次关闭（主窗可能不在前台），
        // 故同一时间只允许一个：已存在/正在初始化则把焦点交回它，不叠开第二个窗口。
        if !self.focus_dialog_if_open(
            DialogType::DrawSettings,
            "音符画设置对话框已打开或在初始化中，忽略重复打开请求",
        ) {
            self.open_dialog_traced(DialogType::DrawSettings, "音符画设置");
        }
    }

    pub(super) fn on_open_shape_drawer(&mut self) {
        if !self.focus_dialog_if_open(
            DialogType::ShapeDrawer,
            "形状抽屉已打开或在初始化中，忽略重复打开请求",
        ) {
            let shapes_dir = crate::storage::config_dir().join("shapes");
            tracing::info!("打开形状抽屉: {}", shapes_dir.display());
            self.window_state
                .dialog_manager
                .open_shape_drawer(shapes_dir);
        }
    }

    pub(super) fn on_open_history(&mut self) {
        if !self.focus_dialog_if_open(
            DialogType::History,
            "撤销历史窗口已打开或在初始化中，忽略重复打开请求",
        ) {
            // 历史窗口打开前先落盘 pending 批量移动/复制，避免用户
            // 刚松开拖动时面板看不到这一步。
            self.window_state.window.ui_mut().flush_history_pending();
            self.open_dialog_traced(DialogType::History, "撤销历史");
        }
    }

    pub(super) fn on_toggle_help(&mut self) {
        if self
            .window_state
            .dialog_manager
            .is_open_or_pending(DialogType::Help)
        {
            self.close_dialog_traced(DialogType::Help, "帮助");
        } else {
            self.open_dialog_traced(DialogType::Help, "帮助");
        }
    }

    pub(super) fn on_open_collaboration(&mut self) {
        self.open_dialog_traced(DialogType::Collaboration, "协作");
    }

    pub(super) fn on_open_project_settings(&mut self) {
        tracing::info!("请求打开工程设置对话框");
        // 优先使用已保存的项目标题，回退到文件名
        let saved_title = self.window_state.window.ui().get_project_settings_title();
        let display_title = if saved_title.is_empty() {
            self.midi_state
                .current_midi_source
                .as_ref()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| tr!("无标题").to_string())
        } else {
            saved_title
        };
        let title = format!("{} - Lumino Midi", display_title);
        self.window_state
            .dialog_manager
            .open_project_settings(title);
    }

    pub(super) fn on_open_port_soundfonts(&mut self) {
        // REND-005：独立窗口；重复打开只聚焦，不叠开第二个。
        if !self.focus_dialog_if_open(
            DialogType::PortSoundfonts,
            "端口音色库窗口已打开或在初始化中，忽略重复打开请求",
        ) {
            self.open_dialog_traced(DialogType::PortSoundfonts, "端口音色库");
        }
    }

    pub(super) fn on_open_speed_change(&mut self) {
        self.open_dialog_traced(DialogType::SpeedChange, "音符变速");
    }

    pub(super) fn on_open_material_delete(&mut self, index: usize, name: String) {
        // DRAW-045：确认框是独立窗口。主窗口保持可交互，重复请求
        // （连点 / 另一个素材）只聚焦已开的窗口，不叠开第二个。
        if self
            .window_state
            .dialog_manager
            .is_open_or_pending(DialogType::MaterialDelete)
        {
            // 主窗口的待确认下标已更新为最新素材：对话框必须同步，
            // 否则确认回执与配对校验不一致，删除会被静默拒绝。
            self.window_state
                .dialog_manager
                .rebind_material_delete(index, name);
            self.focus_dialog_if_open(
                DialogType::MaterialDelete,
                "素材删除确认窗口已打开或在初始化中，改指向最新目标",
            );
        } else {
            self.window_state
                .dialog_manager
                .open_material_delete(index, name);
        }
    }

    pub(super) fn on_open_tempo_max_bpm(&mut self, current_bpm: f64) {
        if !self.focus_dialog_if_open(
            DialogType::TempoMaxBpm,
            "自定义 BPM 上限窗口已打开或在初始化中，忽略重复打开请求",
        ) {
            self.window_state
                .dialog_manager
                .open_tempo_max_bpm(current_bpm);
        }
    }

    pub(super) fn on_open_batch_edit(&mut self) {
        self.open_dialog_traced(DialogType::BatchEdit, "批量编辑");
    }

    pub(super) fn on_open_video_export(&mut self) {
        // CPU 洋葱皮后端：视频导出是 GPU 专属逐帧渲染链路，菜单入口直接拒绝——
        // 开了窗也只会在「开始导出」处失败，不如当下就说清原因。
        if self.reject_video_export_on_cpu() {
            return;
        }
        self.open_dialog_traced(DialogType::VideoExport, "视频导出")
    }

    pub(super) fn on_open_memory_monitor(&mut self) {
        self.open_dialog_traced(DialogType::MemoryMonitor, "内存监控");
    }

    pub(super) fn on_open_load_confirm(&mut self) {}

    pub(super) fn on_open_recover_track(&mut self) {
        tracing::info!("请求打开找回删除音轨对话框");
        // 先扫描缓存目录，把条目列表暂存到 pending_recover_track_entries
        // 对话框 UI 就绪后在 about_to_wait 中通过 try_fill_recover_track_entries 注入
        let entries = self.scan_recover_track_entries();
        tracing::info!("找回删除音轨：扫描到 {} 个已删除音轨缓存", entries.len());
        self.pending_recover_track_entries = Some(entries);
        self.window_state.dialog_manager.open_recover_track();
    }
}
