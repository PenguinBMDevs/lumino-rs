//! Host 混音台窗口相关方法。
//!
//! 混音台是独立对话框窗口（`DialogType::Mixer`）：
//! - 主窗口 Host：提供开关镜像与每帧轨道快照；
//! - 对话框 Host：接收快照、回传动作。

use lumino_ui_egui::mixer::{MixerAction, MixerModel};

use crate::host::Host;

impl Host {
    /// 切换混音台对话框类型（对话框 Host 初始化时调用）。
    pub fn set_mixer_dialog_open(&mut self, open: bool) {
        self.root.set_mixer_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 主窗口是否请求打开混音台独立窗口。
    pub fn mixer_open(&self) -> bool {
        self.root.mixer_open()
    }

    /// 设置主窗口的「打开混音台」镜像开关，并请求重绘（路由栏灯条刷新）。
    pub fn set_mixer_open(&mut self, open: bool) {
        if self.root.mixer_open() == open {
            return;
        }
        self.root.set_mixer_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 主窗口本帧的混音台快照（`None` = 未打开）。
    pub fn mixer_dialog_snapshot(&self) -> Option<MixerModel> {
        self.root.mixer_snapshot()
    }

    /// 注入主窗口推送的快照到混音台对话框；返回是否有变化。
    pub fn set_mixer_dialog_model(&mut self, model: MixerModel) -> bool {
        if !self.root.set_mixer_dialog_model(model) {
            return false;
        }
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
        true
    }

    /// 取走混音台对话框本帧产生的动作（供 Runner 回放到主窗口）。
    pub fn take_mixer_dialog_actions(&mut self) -> Vec<MixerAction> {
        self.root.take_mixer_dialog_actions()
    }

    /// 把对话框动作回放到主窗口（映射为 `SidebarEvent` 并更新状态）。
    pub fn apply_mixer_dialog_actions(&mut self, actions: Vec<MixerAction>) {
        if actions.is_empty() {
            return;
        }
        for action in actions {
            if let Some(message) = self.root.mixer_action_message(&action) {
                let _ = self.process_message(message);
            }
        }
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }
}
