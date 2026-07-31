//! 对话框打开/关闭状态切换

use crate::root::Root;
use crate::state::root_state::DialogType;

impl Root {
    /// 设置自定义精度对话框是否打开
    pub fn set_custom_precision_dialog_open(&mut self, open: bool) {
        self.state.custom_precision_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::CustomPrecision;
        } else if self.state.dialog_type == DialogType::CustomPrecision {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置工程设置对话框是否打开
    pub fn set_project_settings_dialog_open(&mut self, open: bool) {
        self.state.project_settings_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::ProjectSettings;
        } else if self.state.dialog_type == DialogType::ProjectSettings {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置设置对话框是否打开
    pub fn set_settings_dialog_open(&mut self, open: bool) {
        if open {
            self.state.dialog_type = DialogType::Settings;
        } else if self.state.dialog_type == DialogType::Settings {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置音符变速对话框是否打开
    pub fn set_speed_change_dialog_open(&mut self, open: bool) {
        if open {
            self.state.dialog_type = DialogType::SpeedChange;
        } else if self.state.dialog_type == DialogType::SpeedChange {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置批量编辑对话框是否打开
    pub fn set_batch_edit_dialog_open(&mut self, open: bool) {
        self.state.batch_edit_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::BatchEdit;
        } else if self.state.dialog_type == DialogType::BatchEdit {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置导出进度对话框是否打开
    pub fn set_export_progress_dialog_open(&mut self, open: bool) {
        self.state.export_progress_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::ExportProgress;
        } else if self.state.dialog_type == DialogType::ExportProgress {
            self.state.dialog_type = DialogType::None;
        }
    }

    /// 设置内存监控对话框是否打开
    pub fn set_memory_monitor_dialog_open(&mut self, open: bool) {
        self.state.memory_monitor_dialog.is_open = open;
        if open {
            self.state.dialog_type = DialogType::MemoryMonitor;
        } else if self.state.dialog_type == DialogType::MemoryMonitor {
            self.state.dialog_type = DialogType::None;
        }
    }
}
