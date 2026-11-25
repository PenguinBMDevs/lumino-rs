//! 对话框打开/关闭状态切换

use crate::root::Root;
use crate::state::root_state::DialogType;
use lumino_midi_model::multi_port::PortSoundfontMap;

impl Root {
    /// 对话框开关的统一骨架。
    ///
    /// - 打开：设置 `dialog_type`，随后执行 `on_open`（设备扫描 / 状态重置等差异）
    /// - 关闭：**仅当当前对话框就是 `ty` 时**才归零，避免关闭 A 时抹掉刚打开的 B
    fn set_dialog(&mut self, open: bool, ty: DialogType) {
        self.set_dialog_with(open, ty, |_| {});
    }

    /// 带打开副作用（设备扫描 / 数据注入）的对话框开关。
    fn set_dialog_with(&mut self, open: bool, ty: DialogType, on_open: impl FnOnce(&mut Self)) {
        if open {
            self.state.dialog_type = ty;
            on_open(self);
        } else if self.state.dialog_type == ty {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置自定义精度对话框是否打开
    pub fn set_custom_precision_dialog_open(&mut self, open: bool) {
        self.state.custom_precision_dialog.is_open = open;
        self.set_dialog(open, DialogType::CustomPrecision);
    }

    /// 设置工程设置对话框是否打开
    pub fn set_project_settings_dialog_open(&mut self, open: bool) {
        self.state.project_settings_dialog.is_open = open;
        self.set_dialog(open, DialogType::ProjectSettings);
    }

    /// 设置设置对话框是否打开
    ///
    /// 打开时顺手触发两类设备扫描（#149）：真正的 midir 调用在后台线程执行、
    /// 结果经轮询回写本 Root 的设置面板（设置窗为独立 Root，显示的就是这里）；
    /// 非 System 后端由 `scan_winmm_outputs` 内部守卫拦截。输入设备扫描依赖
    /// `sync_midi_state_from` 先注入主窗口的输入 API。
    pub fn set_settings_dialog_open(&mut self, open: bool) {
        self.set_dialog_with(open, DialogType::Settings, |root| {
            root.scan_winmm_outputs();
            root.scan_midi_input_devices();
        });
    }

    /// 设置音符变速对话框是否打开
    pub fn set_speed_change_dialog_open(&mut self, open: bool) {
        self.set_dialog(open, DialogType::SpeedChange);
    }

    /// 注入素材删除确认对话框的数据并打开（独立 OS 窗口；DRAW-045）
    ///
    /// 素材名是打开时的快照：列表在对话框展示期间刷新也不影响文案。
    pub fn set_material_delete_dialog(&mut self, index: usize, name: String) {
        self.state.material_delete_dialog.open(index, name);
        self.state.dialog_type = DialogType::MaterialDelete;
    }

    /// 注入自定义 BPM 上限对话框的当前值并打开（独立 OS 窗口；DRAW-045）
    pub fn set_tempo_max_bpm_dialog(&mut self, current_bpm: f64) {
        self.state.tempo_max_bpm_dialog.open(current_bpm);
        self.state.dialog_type = DialogType::TempoMaxBpm;
    }

    /// 注入端口音色库映射并打开对话框（REND-005，独立 OS 窗口）。
    ///
    /// `document_max_port` 决定展示到第几个端口；已配置但超出当前布局的端口
    /// 仍会展示（由状态层保证），避免“打开再确定”丢配置。
    pub fn set_port_soundfonts_dialog(&mut self, map: &PortSoundfontMap, document_max_port: u8) {
        self.state
            .port_soundfonts_dialog
            .open(map, document_max_port);
        self.state.dialog_type = DialogType::PortSoundfonts;
    }

    /// 设置批量编辑对话框是否打开
    pub fn set_batch_edit_dialog_open(&mut self, open: bool) {
        self.state.batch_edit_dialog.is_open = open;
        self.set_dialog(open, DialogType::BatchEdit);
    }

    /// 设置导出进度对话框是否打开
    pub fn set_export_progress_dialog_open(&mut self, open: bool) {
        self.state.export_progress_dialog.is_open = open;
        self.set_dialog(open, DialogType::ExportProgress);
    }

    /// 设置内存监控对话框是否打开
    pub fn set_memory_monitor_dialog_open(&mut self, open: bool) {
        self.state.memory_monitor_dialog.is_open = open;
        self.set_dialog(open, DialogType::MemoryMonitor);
    }

    /// 设置找回删除音轨对话框是否打开
    pub fn set_recover_track_dialog_open(&mut self, open: bool) {
        self.state.recover_track_dialog.is_open = open;
        self.set_dialog_with(open, DialogType::RecoverTrack, |root| {
            // 打开时重置选中状态，避免上次的选中项在新列表中越界
            root.state.recover_track_dialog.selected_index = None;
        });
    }

    /// 设置保存确认对话框是否打开
    pub fn set_save_confirm_dialog_open(&mut self, open: bool) {
        self.state.save_confirm_dialog.is_open = open;
        self.set_dialog(open, DialogType::SaveConfirm);
    }

    /// 设置自动保存恢复对话框是否打开（独立对话框窗口；DEBT-12）
    pub fn set_autosave_recover_dialog_open(&mut self, open: bool) {
        self.state.autosave_recover_dialog.is_open = open;
        self.set_dialog(open, DialogType::AutosaveRecover);
    }

    /// 设置「音符画设置」对话框是否打开（独立 OS 窗口）
    ///
    /// 无独立状态位：该对话框本轮为占位面板，`dialog_type` 本身就是"开/关"的唯一来源
    /// （其余对话框额外带 `is_open` 是因为面板内还挂着草稿数据）。
    pub fn set_draw_settings_dialog_open(&mut self, open: bool) {
        self.set_dialog(open, DialogType::DrawSettings);
    }

    /// 设置 Help 窗口是否打开（独立 OS 窗口；DRAW-038）
    pub fn set_help_dialog_open(&mut self, open: bool) {
        self.set_dialog(open, DialogType::Help);
    }

    /// 设置形状抽屉是否打开（独立 OS 窗口；DRAW-033）
    pub fn set_shape_drawer_dialog_open(&mut self, open: bool) {
        self.set_dialog(open, DialogType::ShapeDrawer);
    }

    /// 设置混音台窗口是否打开（独立 OS 窗口）
    pub fn set_mixer_dialog_open(&mut self, open: bool) {
        self.set_dialog(open, DialogType::Mixer);
    }

    /// 设置撤销历史窗口是否打开（独立 OS 窗口；DRAW-035）
    pub fn set_history_dialog_open(&mut self, open: bool) {
        self.set_dialog(open, DialogType::History);
    }

    /// 用编辑器历史快照刷新历史面板；返回是否有变化。
    pub fn set_history_view(
        &mut self,
        steps: Vec<crate::state::root_state::HistoryStepView>,
        current: usize,
    ) -> bool {
        self.state.history_dialog.replace(steps, current)
    }

    /// 设置找回删除音轨对话框的条目列表（由 Runner 扫描缓存目录后填充）
    pub fn set_recover_track_dialog_entries(
        &mut self,
        entries: Vec<crate::state::root_state::RecoverTrackEntry>,
    ) {
        self.state.recover_track_dialog.entries = entries;
        // 默认选中第一个条目（如果有）
        self.state.recover_track_dialog.selected_index =
            if self.state.recover_track_dialog.entries.is_empty() {
                None
            } else {
                Some(0)
            };
    }
}
