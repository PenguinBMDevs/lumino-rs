//! `DialogManager` 查询与状态同步实现 — 存在性/引用查询、主题与云状态广播
//!
//! 自 `manager.rs` 拆分而来（零逻辑变更）。

use lumino_ui::state::root_state::DialogType;
use winit::window::WindowId;

use crate::window::DialogWindow;

use super::DialogManager;

impl DialogManager {
    /// 检查是否是对话框窗口
    pub fn is_dialog_window(&self, window_id: WindowId) -> bool {
        self.dialogs.contains_key(&window_id)
    }

    /// 获取对话框的可变引用
    pub fn get_dialog_mut(&mut self, window_id: WindowId) -> Option<&mut DialogWindow> {
        self.dialogs.get_mut(&window_id)
    }
}

impl DialogManager {
    /// 检查指定类型的对话框是否存在
    pub fn has_dialog_type(&self, dialog_type: DialogType) -> bool {
        self.dialogs.values().any(|d| d.dialog_type == dialog_type)
    }

    /// 将主窗口的云存储 UI 状态广播到所有相关对话框
    ///
    /// 云存储唯一数据源是主窗口 Root（连接快照/目录列表/提醒由 runner 注入），
    /// 设置面板云管理页与云文件浏览器为独立 Root，需在此同步最新快照，
    /// 否则对话框内看不到已连接的设备。
    /// 使用**共享快照**同步（排除连接表单字段），避免覆盖用户正在输入的内容。
    pub fn sync_cloud_to_dialogs(&mut self, main_ui: &lumino_ui::Host) {
        for dialog in self.dialogs.values_mut() {
            let relevant = matches!(
                dialog.dialog_type,
                DialogType::Settings
                    | DialogType::CloudConnect
                    | DialogType::CloudBrowser
                    | DialogType::CloudNotice
            );
            if relevant && let Some(ui) = dialog.ui_mut() {
                ui.sync_cloud_snapshot_from(main_ui);
            }
        }
    }

    /// 将 GPU 兼容性检测结果注入所有已打开的设置对话框。
    ///
    /// 返回是否至少注入了一个对话框：无设置对话框时结果由 Runner 暂存，
    /// 待下次设置对话框就绪后再注入（与 RecoverTrack 条目的 pending 模式一致）。
    pub fn apply_gpu_check_result(&mut self, passed: bool, detail: String) -> bool {
        let mut applied = false;
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::Settings
                && let Some(ui) = dialog.ui_mut()
            {
                ui.set_gpu_check_result(passed, detail.clone());
                dialog.window().request_redraw();
                applied = true;
            }
        }
        applied
    }

    /// 将主题同步到所有已打开的对话框窗口
    ///
    /// 对话框主题在创建时从配置快照读取，运行中切换主题后若不广播，
    /// 对话框仍使用旧主题渲染（背景与文字可能与新主题不一致）。
    pub fn update_theme_all(&mut self, theme: String) {
        for dialog in self.dialogs.values_mut() {
            if let Some(ui) = dialog.ui_mut() {
                ui.update_theme(theme.clone());
            }
        }
    }

    /// 取出设置对话框内发起、待全局应用的主题。
    ///
    /// 设置面板切换主题后需立即同步主窗口与所有对话框（而非等确认按钮），
    /// Runner 在 `about_to_wait` 中逐帧调用本方法消费。
    pub fn take_pending_settings_theme(&mut self) -> Option<String> {
        self.dialogs
            .values_mut()
            .filter(|d| d.dialog_type == DialogType::Settings)
            .find_map(|d| d.take_pending_theme_apply())
    }

    /// 查找指定类型的第一个对话框窗口 ID
    ///
    /// 用于 Runner 在对话框 UI 就绪后注入数据（如 RecoverTrack 对话框的条目列表）。
    /// 返回 `None` 表示该类型对话框尚未就绪（仍在初始化或不存在）。
    pub fn first_dialog_id_of_type(&self, dialog_type: DialogType) -> Option<WindowId> {
        self.dialogs
            .values()
            .find(|d| d.dialog_type == dialog_type)
            .map(|d| d.window_id())
    }

    /// 获取指定类型对话框的窗口引用（若已就绪）
    ///
    /// 用于悬浮窗定位（如云传输进度窗覆盖在云浏览对话框上）。
    pub fn dialog_window_of_type(
        &self,
        dialog_type: DialogType,
    ) -> Option<&std::sync::Arc<winit::window::Window>> {
        self.dialogs
            .values()
            .find(|d| d.dialog_type == dialog_type)
            .map(|d| d.window())
    }
}
